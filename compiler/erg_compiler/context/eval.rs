use std::fmt;
use std::mem;

use erg_common::dict::Dict;
use erg_common::error::Location;
use erg_common::set::Set;
use erg_common::shared::Shared;
use erg_common::traits::{Locational, Stream};
use erg_common::vis::Field;
use erg_common::{dict, fn_name, option_enum_unwrap, set};
use erg_common::{RcArray, Str};
use OpKind::*;

use erg_parser::ast::*;
use erg_parser::token::{Token, TokenKind};

use erg_type::constructors::{
    and, builtin_mono, builtin_poly, mono_proj, not, or, poly, ref_, ref_mut, refinement, subr_t,
    v_enum,
};
use erg_type::typaram::{OpKind, TyParam};
use erg_type::value::ValueObj;
use erg_type::{
    ConstSubr, HasType, ParamTy, Predicate, SubrKind, TyBound, Type, UserConstSubr, ValueArgs,
};

use crate::context::instantiate::TyVarContext;
use crate::context::{ClassDefType, Context, ContextKind, RegistrationMode};
use crate::error::{EvalError, EvalErrors, EvalResult, SingleEvalResult, TyCheckResult};

#[inline]
pub fn type_from_token_kind(kind: TokenKind) -> Type {
    use TokenKind::*;

    match kind {
        NatLit => Type::Nat,
        IntLit => Type::Int,
        RatioLit => Type::Ratio,
        StrLit => Type::Str,
        BoolLit => Type::Bool,
        NoneLit => Type::NoneType,
        NoImplLit => Type::NotImplemented,
        EllipsisLit => Type::Ellipsis,
        InfLit => Type::Inf,
        other => panic!("this has not type: {other}"),
    }
}

fn try_get_op_kind_from_token(kind: TokenKind) -> EvalResult<OpKind> {
    match kind {
        TokenKind::Plus => Ok(OpKind::Add),
        TokenKind::Minus => Ok(OpKind::Sub),
        TokenKind::Star => Ok(OpKind::Mul),
        TokenKind::Slash => Ok(OpKind::Div),
        TokenKind::Pow => Ok(OpKind::Pow),
        TokenKind::Mod => Ok(OpKind::Mod),
        TokenKind::DblEq => Ok(OpKind::Eq),
        TokenKind::NotEq => Ok(OpKind::Ne),
        TokenKind::Less => Ok(OpKind::Lt),
        TokenKind::Gre => Ok(OpKind::Gt),
        TokenKind::LessEq => Ok(OpKind::Le),
        TokenKind::GreEq => Ok(OpKind::Ge),
        TokenKind::AndOp => Ok(OpKind::And),
        TokenKind::OrOp => Ok(OpKind::Or),
        TokenKind::BitAnd => Ok(OpKind::BitAnd),
        TokenKind::BitXor => Ok(OpKind::BitXor),
        TokenKind::BitOr => Ok(OpKind::BitOr),
        TokenKind::Shl => Ok(OpKind::Shl),
        TokenKind::Shr => Ok(OpKind::Shr),
        TokenKind::Mutate => Ok(OpKind::Mutate),
        _other => todo!("{_other}"),
    }
}

#[inline]
pub(crate) fn eval_lit(lit: &Literal) -> ValueObj {
    let t = type_from_token_kind(lit.token.kind);
    ValueObj::from_str(t, lit.token.content.clone())
}

/// SubstContext::new(Array(?T, 0), Context(Array('T, 'N))) => SubstContext{ params: { 'T: ?T; 'N: 0 } } => ctx
/// ctx.substitute(Array!('T; !'N)): Array(?T, !0)
#[derive(Debug)]
pub struct SubstContext {
    bounds: Set<TyBound>,
    params: Dict<Str, TyParam>,
}

impl fmt::Display for SubstContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SubstContext{{ bounds: {}, params: {} }}",
            self.bounds, self.params
        )
    }
}

impl SubstContext {
    pub fn new(substituted: &Type, ty_ctx: &Context) -> Self {
        let bounds = ty_ctx.type_params_bounds();
        let param_names = ty_ctx.params.iter().map(|(opt_name, _)| {
            opt_name
                .as_ref()
                .map_or_else(|| Str::ever("_"), |n| n.inspect().clone())
        });
        assert_eq!(param_names.len(), substituted.typarams().len());
        // REVIEW: 順番は保証されるか? 引数がunnamed_paramsに入る可能性は?
        SubstContext {
            bounds,
            params: param_names
                .zip(substituted.typarams().into_iter())
                .collect(),
        }
    }

    pub fn substitute(&self, quant_t: Type, ctx: &Context, loc: Location) -> TyCheckResult<Type> {
        let tv_ctx = TyVarContext::new(ctx.level, self.bounds.clone(), ctx);
        let inst = ctx.instantiate_t(quant_t, &tv_ctx, loc)?;
        for param in inst.typarams() {
            self.substitute_tp(&param, ctx)?;
        }
        Ok(inst)
    }

    fn substitute_tp(&self, param: &TyParam, ctx: &Context) -> TyCheckResult<()> {
        match param {
            TyParam::FreeVar(fv) => {
                if let Some(name) = fv.unbound_name() {
                    if let Some(tp) = self.params.get(&name) {
                        ctx.sub_unify_tp(param, tp, None, Location::Unknown, false)?;
                    }
                } else if fv.is_unbound() {
                    panic!()
                }
            }
            TyParam::BinOp { lhs, rhs, .. } => {
                self.substitute_tp(lhs, ctx)?;
                self.substitute_tp(rhs, ctx)?;
            }
            TyParam::UnaryOp { val, .. } => {
                self.substitute_tp(val, ctx)?;
            }
            TyParam::Array(args)
            | TyParam::Tuple(args)
            | TyParam::App { args, .. }
            | TyParam::PolyQVar { args, .. } => {
                for arg in args.iter() {
                    self.substitute_tp(arg, ctx)?;
                }
            }
            TyParam::Type(t) => {
                self.substitute_t(t, ctx)?;
            }
            TyParam::MonoProj { obj, .. } => {
                self.substitute_tp(obj, ctx)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn substitute_t(&self, param_t: &Type, ctx: &Context) -> TyCheckResult<()> {
        match param_t {
            Type::FreeVar(fv) => {
                if let Some(name) = fv.unbound_name() {
                    if let Some(tp) = self.params.get(&name) {
                        if let TyParam::Type(t) = tp {
                            ctx.sub_unify(param_t, t, Location::Unknown, None)?;
                        } else {
                            panic!()
                        }
                    }
                }
            }
            Type::Subr(subr) => {
                for nd_param in subr.non_default_params.iter() {
                    self.substitute_t(nd_param.typ(), ctx)?;
                }
                if let Some(var_params) = &subr.var_params {
                    self.substitute_t(var_params.typ(), ctx)?;
                }
                for d_param in subr.default_params.iter() {
                    self.substitute_t(d_param.typ(), ctx)?;
                }
                self.substitute_t(&subr.return_t, ctx)?;
            }
            Type::And(l, r) | Type::Or(l, r) | Type::Not(l, r) => {
                self.substitute_t(l, ctx)?;
                self.substitute_t(r, ctx)?;
            }
            Type::MonoProj { lhs, .. } => {
                self.substitute_t(lhs, ctx)?;
            }
            Type::Record(rec) => {
                for (_, t) in rec.iter() {
                    self.substitute_t(t, ctx)?;
                }
            }
            Type::Ref(t) => {
                self.substitute_t(t, ctx)?;
            }
            Type::RefMut { before, after } => {
                self.substitute_t(before, ctx)?;
                if let Some(aft) = after {
                    self.substitute_t(aft, ctx)?;
                }
            }
            Type::Refinement(refine) => {
                self.substitute_t(&refine.t, ctx)?;
            }
            Type::Poly { params, .. }
            | Type::BuiltinPoly { params, .. }
            | Type::PolyQVar { params, .. } => {
                for param in params.iter() {
                    self.substitute_tp(param, ctx)?;
                }
            }
            t => todo!("{t:?}"),
        }
        Ok(())
    }
}

impl Context {
    fn eval_const_acc(&self, acc: &Accessor) -> EvalResult<ValueObj> {
        match acc {
            Accessor::Ident(ident) => {
                if let Some(val) = self.rec_get_const_obj(ident.inspect()) {
                    Ok(val.clone())
                } else if ident.is_const() {
                    Err(EvalErrors::from(EvalError::no_var_error(
                        self.cfg.input.clone(),
                        line!() as usize,
                        ident.loc(),
                        self.caused_by(),
                        ident.inspect(),
                        self.get_similar_name(ident.inspect()),
                    )))
                } else {
                    Err(EvalErrors::from(EvalError::not_const_expr(
                        self.cfg.input.clone(),
                        line!() as usize,
                        acc.loc(),
                        self.caused_by(),
                    )))
                }
            }
            Accessor::Attr(attr) => {
                let obj = self.eval_const_expr(&attr.obj, None)?;
                Ok(self.eval_attr(obj, &attr.ident)?)
            }
            _ => todo!(),
        }
    }

    fn eval_attr(&self, obj: ValueObj, ident: &Identifier) -> SingleEvalResult<ValueObj> {
        if let Some(val) = obj.try_get_attr(&Field::from(ident)) {
            return Ok(val);
        }
        if let ValueObj::Type(t) = &obj {
            if let Some(sups) = self.get_nominal_super_type_ctxs(t.typ()) {
                for (_, ctx) in sups {
                    if let Some(val) = ctx.consts.get(ident.inspect()) {
                        return Ok(val.clone());
                    }
                    for (_, methods) in ctx.methods_list.iter() {
                        if let Some(v) = methods.consts.get(ident.inspect()) {
                            return Ok(v.clone());
                        }
                    }
                }
            }
        }
        Err(EvalError::no_attr_error(
            self.cfg.input.clone(),
            line!() as usize,
            ident.loc(),
            self.caused_by(),
            &obj.t(),
            ident.inspect(),
            None,
        ))
    }

    fn eval_const_bin(&self, bin: &BinOp) -> EvalResult<ValueObj> {
        let lhs = self.eval_const_expr(&bin.args[0], None)?;
        let rhs = self.eval_const_expr(&bin.args[1], None)?;
        let op = try_get_op_kind_from_token(bin.op.kind)?;
        self.eval_bin(op, lhs, rhs)
    }

    fn eval_const_unary(&self, unary: &UnaryOp) -> EvalResult<ValueObj> {
        let val = self.eval_const_expr(&unary.args[0], None)?;
        let op = try_get_op_kind_from_token(unary.op.kind)?;
        self.eval_unary(op, val)
    }

    fn eval_args(&self, args: &Args, __name__: Option<&Str>) -> EvalResult<ValueArgs> {
        let mut evaluated_pos_args = vec![];
        for arg in args.pos_args().iter() {
            let val = self.eval_const_expr(&arg.expr, __name__)?;
            evaluated_pos_args.push(val);
        }
        let mut evaluated_kw_args = dict! {};
        for arg in args.kw_args().iter() {
            let val = self.eval_const_expr(&arg.expr, __name__)?;
            evaluated_kw_args.insert(arg.keyword.inspect().clone(), val);
        }
        Ok(ValueArgs::new(evaluated_pos_args, evaluated_kw_args))
    }

    fn eval_const_call(&self, call: &Call, __name__: Option<&Str>) -> EvalResult<ValueObj> {
        if let Expr::Accessor(acc) = call.obj.as_ref() {
            match acc {
                Accessor::Ident(ident) => {
                    let obj = self.rec_get_const_obj(ident.inspect()).ok_or_else(|| {
                        EvalError::no_var_error(
                            self.cfg.input.clone(),
                            line!() as usize,
                            ident.loc(),
                            self.caused_by(),
                            ident.inspect(),
                            self.get_similar_name(ident.inspect()),
                        )
                    })?;
                    let subr = option_enum_unwrap!(obj, ValueObj::Subr)
                        .ok_or_else(|| {
                            EvalError::type_mismatch_error(
                                self.cfg.input.clone(),
                                line!() as usize,
                                ident.loc(),
                                self.caused_by(),
                                ident.inspect(),
                                &builtin_mono("Subroutine"),
                                &obj.t(),
                                self.get_candidates(&obj.t()),
                                None,
                            )
                        })?
                        .clone();
                    let args = self.eval_args(&call.args, __name__)?;
                    self.call(subr, args, __name__.cloned(), call.loc())
                }
                Accessor::Attr(_attr) => todo!(),
                Accessor::TupleAttr(_attr) => todo!(),
                Accessor::Subscr(_subscr) => todo!(),
                Accessor::TypeApp(_type_app) => todo!(),
            }
        } else {
            todo!()
        }
    }

    fn call(
        &self,
        subr: ConstSubr,
        args: ValueArgs,
        __name__: Option<Str>,
        loc: Location,
    ) -> EvalResult<ValueObj> {
        match subr {
            ConstSubr::User(_user) => todo!(),
            ConstSubr::Builtin(builtin) => builtin
                .call(args, self.path().to_path_buf(), __name__)
                .map_err(|mut e| {
                    e.loc = loc;
                    EvalErrors::from(EvalError::new(e, self.cfg.input.clone(), self.caused_by()))
                }),
        }
    }

    fn eval_const_def(&mut self, def: &Def) -> EvalResult<ValueObj> {
        if def.is_const() {
            let __name__ = def.sig.ident().unwrap().inspect();
            let vis = def.sig.vis();
            let tv_ctx = match &def.sig {
                Signature::Subr(subr) => {
                    let bounds =
                        self.instantiate_ty_bounds(&subr.bounds, RegistrationMode::Normal)?;
                    Some(TyVarContext::new(self.level, bounds, self))
                }
                Signature::Var(_) => None,
            };
            self.grow(__name__, ContextKind::Instant, vis, tv_ctx)?;
            let obj = self
                .eval_const_block(&def.body.block, Some(__name__))
                .map_err(|e| {
                    self.pop();
                    e
                })?;
            match self.check_decls_and_pop() {
                Ok(_) => {
                    self.register_gen_const(def.sig.ident().unwrap(), obj)?;
                    Ok(ValueObj::None)
                }
                Err(errs) => {
                    self.register_gen_const(def.sig.ident().unwrap(), obj)?;
                    Err(errs)
                }
            }
        } else {
            Err(EvalErrors::from(EvalError::not_const_expr(
                self.cfg.input.clone(),
                line!() as usize,
                def.body.block.loc(),
                self.caused_by(),
            )))
        }
    }

    fn eval_const_array(&self, arr: &Array) -> EvalResult<ValueObj> {
        let mut elems = vec![];
        match arr {
            Array::Normal(arr) => {
                for elem in arr.elems.pos_args().iter() {
                    let elem = self.eval_const_expr(&elem.expr, None)?;
                    elems.push(elem);
                }
            }
            _ => {
                todo!()
            }
        }
        Ok(ValueObj::Array(RcArray::from(elems)))
    }

    fn eval_const_record(&self, record: &Record) -> EvalResult<ValueObj> {
        match record {
            Record::Normal(rec) => self.eval_const_normal_record(rec),
            Record::Shortened(_rec) => unreachable!(), // should be desugared
        }
    }

    fn eval_const_normal_record(&self, record: &NormalRecord) -> EvalResult<ValueObj> {
        let mut attrs = vec![];
        // HACK: should avoid cloning
        let mut record_ctx = Context::instant(
            Str::ever("<unnamed record>"),
            self.cfg.clone(),
            2,
            self.mod_cache.clone(),
            self.py_mod_cache.clone(),
            self.clone(),
        );
        for attr in record.attrs.iter() {
            let name = attr.sig.ident().map(|i| i.inspect());
            let elem = record_ctx.eval_const_block(&attr.body.block, name)?;
            let ident = match &attr.sig {
                Signature::Var(var) => match &var.pat {
                    VarPattern::Ident(ident) => Field::new(ident.vis(), ident.inspect().clone()),
                    _ => todo!(),
                },
                _ => todo!(),
            };
            attrs.push((ident, elem));
        }
        Ok(ValueObj::Record(attrs.into_iter().collect()))
    }

    /// FIXME: grow
    fn eval_const_lambda(&self, lambda: &Lambda) -> EvalResult<ValueObj> {
        let bounds = self.instantiate_ty_bounds(&lambda.sig.bounds, RegistrationMode::Normal)?;
        let tv_ctx = TyVarContext::new(self.level, bounds, self);
        let mut non_default_params = Vec::with_capacity(lambda.sig.params.non_defaults.len());
        for sig in lambda.sig.params.non_defaults.iter() {
            let t =
                self.instantiate_param_sig_t(sig, None, Some(&tv_ctx), RegistrationMode::Normal)?;
            let pt = if let Some(name) = sig.inspect() {
                ParamTy::kw(name.clone(), t)
            } else {
                ParamTy::anonymous(t)
            };
            non_default_params.push(pt);
        }
        let var_params = if let Some(p) = lambda.sig.params.var_args.as_ref() {
            let t =
                self.instantiate_param_sig_t(p, None, Some(&tv_ctx), RegistrationMode::Normal)?;
            let pt = if let Some(name) = p.inspect() {
                ParamTy::kw(name.clone(), t)
            } else {
                ParamTy::anonymous(t)
            };
            Some(pt)
        } else {
            None
        };
        let mut default_params = Vec::with_capacity(lambda.sig.params.defaults.len());
        for sig in lambda.sig.params.defaults.iter() {
            let t =
                self.instantiate_param_sig_t(sig, None, Some(&tv_ctx), RegistrationMode::Normal)?;
            let pt = if let Some(name) = sig.inspect() {
                ParamTy::kw(name.clone(), t)
            } else {
                ParamTy::anonymous(t)
            };
            default_params.push(pt);
        }
        // HACK: should avoid cloning
        let mut lambda_ctx = Context::instant(
            Str::ever("<lambda>"),
            self.cfg.clone(),
            0,
            self.mod_cache.clone(),
            self.py_mod_cache.clone(),
            self.clone(),
        );
        let return_t = lambda_ctx.eval_const_block(&lambda.body, None)?;
        // FIXME: lambda: i: Int -> Int
        // => sig_t: (i: Type) -> Type
        // => as_type: (i: Int) -> Int
        let sig_t = subr_t(
            SubrKind::from(lambda.op.kind),
            non_default_params.clone(),
            var_params.clone(),
            default_params.clone(),
            v_enum(set![return_t.clone()]),
        );
        let sig_t = self.generalize_t(sig_t);
        let as_type = subr_t(
            SubrKind::from(lambda.op.kind),
            non_default_params,
            var_params,
            default_params,
            // TODO: unwrap
            return_t.as_type().unwrap().into_typ(),
        );
        let as_type = self.generalize_t(as_type);
        let subr = ConstSubr::User(UserConstSubr::new(
            Str::ever("<lambda>"),
            lambda.sig.params.clone(),
            lambda.body.clone(),
            sig_t,
            Some(as_type),
        ));
        Ok(ValueObj::Subr(subr))
    }

    pub(crate) fn eval_const_expr(
        &self,
        expr: &Expr,
        __name__: Option<&Str>,
    ) -> EvalResult<ValueObj> {
        match expr {
            Expr::Lit(lit) => Ok(eval_lit(lit)),
            Expr::Accessor(acc) => self.eval_const_acc(acc),
            Expr::BinOp(bin) => self.eval_const_bin(bin),
            Expr::UnaryOp(unary) => self.eval_const_unary(unary),
            Expr::Call(call) => self.eval_const_call(call, __name__),
            Expr::Array(arr) => self.eval_const_array(arr),
            Expr::Record(rec) => self.eval_const_record(rec),
            Expr::Lambda(lambda) => self.eval_const_lambda(lambda),
            other => todo!("{other}"),
        }
    }

    // ConstExprを評価するのではなく、コンパイル時関数の式(AST上ではただのExpr)を評価する
    // コンパイル時評価できないならNoneを返す
    pub(crate) fn eval_const_chunk(
        &mut self,
        expr: &Expr,
        __name__: Option<&Str>,
    ) -> EvalResult<ValueObj> {
        match expr {
            Expr::Lit(lit) => Ok(eval_lit(lit)),
            Expr::Accessor(acc) => self.eval_const_acc(acc),
            Expr::BinOp(bin) => self.eval_const_bin(bin),
            Expr::UnaryOp(unary) => self.eval_const_unary(unary),
            Expr::Call(call) => self.eval_const_call(call, __name__),
            Expr::Def(def) => self.eval_const_def(def),
            Expr::Array(arr) => self.eval_const_array(arr),
            Expr::Record(rec) => self.eval_const_record(rec),
            Expr::Lambda(lambda) => self.eval_const_lambda(lambda),
            other => todo!("{other}"),
        }
    }

    pub(crate) fn eval_const_block(
        &mut self,
        block: &Block,
        __name__: Option<&Str>,
    ) -> EvalResult<ValueObj> {
        for chunk in block.iter().rev().skip(1).rev() {
            self.eval_const_chunk(chunk, __name__)?;
        }
        self.eval_const_chunk(block.last().unwrap(), __name__)
    }

    fn eval_bin(&self, op: OpKind, lhs: ValueObj, rhs: ValueObj) -> EvalResult<ValueObj> {
        match op {
            Add => lhs.try_add(rhs).ok_or_else(|| {
                EvalErrors::from(EvalError::unreachable(
                    self.cfg.input.clone(),
                    fn_name!(),
                    line!(),
                ))
            }),
            Sub => lhs.try_sub(rhs).ok_or_else(|| {
                EvalErrors::from(EvalError::unreachable(
                    self.cfg.input.clone(),
                    fn_name!(),
                    line!(),
                ))
            }),
            Mul => lhs.try_mul(rhs).ok_or_else(|| {
                EvalErrors::from(EvalError::unreachable(
                    self.cfg.input.clone(),
                    fn_name!(),
                    line!(),
                ))
            }),
            Div => lhs.try_div(rhs).ok_or_else(|| {
                EvalErrors::from(EvalError::unreachable(
                    self.cfg.input.clone(),
                    fn_name!(),
                    line!(),
                ))
            }),
            Gt => lhs.try_gt(rhs).ok_or_else(|| {
                EvalErrors::from(EvalError::unreachable(
                    self.cfg.input.clone(),
                    fn_name!(),
                    line!(),
                ))
            }),
            Ge => lhs.try_ge(rhs).ok_or_else(|| {
                EvalErrors::from(EvalError::unreachable(
                    self.cfg.input.clone(),
                    fn_name!(),
                    line!(),
                ))
            }),
            Eq => lhs.try_eq(rhs).ok_or_else(|| {
                EvalErrors::from(EvalError::unreachable(
                    self.cfg.input.clone(),
                    fn_name!(),
                    line!(),
                ))
            }),
            Ne => lhs.try_ne(rhs).ok_or_else(|| {
                EvalErrors::from(EvalError::unreachable(
                    self.cfg.input.clone(),
                    fn_name!(),
                    line!(),
                ))
            }),
            other => todo!("{other}"),
        }
    }

    pub(crate) fn eval_bin_tp(
        &self,
        op: OpKind,
        lhs: &TyParam,
        rhs: &TyParam,
    ) -> EvalResult<TyParam> {
        match (lhs, rhs) {
            (TyParam::Value(ValueObj::Mut(lhs)), TyParam::Value(rhs)) => self
                .eval_bin(op, lhs.borrow().clone(), rhs.clone())
                .map(|v| TyParam::Value(ValueObj::Mut(Shared::new(v)))),
            (TyParam::Value(lhs), TyParam::Value(rhs)) => self
                .eval_bin(op, lhs.clone(), rhs.clone())
                .map(TyParam::value),
            (TyParam::FreeVar(fv), r) => {
                if fv.is_linked() {
                    self.eval_bin_tp(op, &*fv.crack(), r)
                } else {
                    Err(EvalErrors::from(EvalError::unreachable(
                        self.cfg.input.clone(),
                        fn_name!(),
                        line!(),
                    )))
                }
            }
            (l, TyParam::FreeVar(fv)) => {
                if fv.is_linked() {
                    self.eval_bin_tp(op, l, &*fv.crack())
                } else {
                    Err(EvalErrors::from(EvalError::unreachable(
                        self.cfg.input.clone(),
                        fn_name!(),
                        line!(),
                    )))
                }
            }
            (e @ TyParam::Erased(_), _) | (_, e @ TyParam::Erased(_)) => Ok(e.clone()),
            (l, r) => todo!("{l} {op} {r}"),
        }
    }

    fn eval_unary(&self, op: OpKind, val: ValueObj) -> EvalResult<ValueObj> {
        match op {
            Pos => todo!(),
            Neg => todo!(),
            Invert => todo!(),
            Mutate => Ok(ValueObj::Mut(Shared::new(val))),
            other => todo!("{other}"),
        }
    }

    fn eval_unary_tp(&self, op: OpKind, val: &TyParam) -> EvalResult<TyParam> {
        match val {
            TyParam::Value(c) => self.eval_unary(op, c.clone()).map(TyParam::Value),
            TyParam::FreeVar(fv) if fv.is_linked() => self.eval_unary_tp(op, &*fv.crack()),
            e @ TyParam::Erased(_) => Ok(e.clone()),
            other => todo!("{op} {other}"),
        }
    }

    fn eval_app(&self, _name: &Str, _args: &[TyParam]) -> EvalResult<TyParam> {
        todo!()
    }

    /// 量化変数などはそのまま返す
    pub(crate) fn eval_tp(&self, p: &TyParam) -> EvalResult<TyParam> {
        match p {
            TyParam::FreeVar(fv) if fv.is_linked() => self.eval_tp(&fv.crack()),
            TyParam::Mono(name) => self
                .rec_get_const_obj(name)
                .map(|v| TyParam::value(v.clone()))
                .ok_or_else(|| {
                    EvalErrors::from(EvalError::unreachable(
                        self.cfg.input.clone(),
                        fn_name!(),
                        line!(),
                    ))
                }),
            TyParam::BinOp { op, lhs, rhs } => self.eval_bin_tp(*op, lhs, rhs),
            TyParam::UnaryOp { op, val } => self.eval_unary_tp(*op, val),
            TyParam::App { name, args } => self.eval_app(name, args),
            p @ (TyParam::Type(_)
            | TyParam::Erased(_)
            | TyParam::Value(_)
            | TyParam::FreeVar(_)
            | TyParam::MonoQVar(_)) => Ok(p.clone()),
            _other => Err(EvalErrors::from(EvalError::feature_error(
                self.cfg.input.clone(),
                Location::Unknown,
                "???",
                self.caused_by(),
            ))),
        }
    }

    pub(crate) fn eval_t_params(
        &self,
        substituted: Type,
        level: usize,
        t_loc: Location,
    ) -> EvalResult<Type> {
        match substituted {
            Type::FreeVar(fv) if fv.is_linked() => {
                self.eval_t_params(fv.crack().clone(), level, t_loc)
            }
            Type::Subr(mut subr) => {
                for pt in subr.non_default_params.iter_mut() {
                    *pt.typ_mut() = self.eval_t_params(mem::take(pt.typ_mut()), level, t_loc)?;
                }
                if let Some(var_args) = subr.var_params.as_mut() {
                    *var_args.typ_mut() =
                        self.eval_t_params(mem::take(var_args.typ_mut()), level, t_loc)?;
                }
                for pt in subr.default_params.iter_mut() {
                    *pt.typ_mut() = self.eval_t_params(mem::take(pt.typ_mut()), level, t_loc)?;
                }
                let return_t = self.eval_t_params(*subr.return_t, level, t_loc)?;
                Ok(subr_t(
                    subr.kind,
                    subr.non_default_params,
                    subr.var_params.map(|v| *v),
                    subr.default_params,
                    return_t,
                ))
            }
            Type::Refinement(refine) => {
                let mut preds = Set::with_capacity(refine.preds.len());
                for pred in refine.preds.into_iter() {
                    preds.insert(self.eval_pred(pred)?);
                }
                Ok(refinement(refine.var, *refine.t, preds))
            }
            // [?T; 0].MutType! == [?T; !0]
            // ?T(<: Add(?R(:> Int))).Output == ?T(<: Add(?R)).Output
            // ?T(:> Int, <: Add(?R(:> Int))).Output == Int
            Type::MonoProj { lhs, rhs } => {
                // Currently Erg does not allow projection-types to be evaluated with type variables included.
                // All type variables will be dereferenced or fail.
                let (sub, opt_sup) = match *lhs.clone() {
                    Type::FreeVar(fv) if fv.is_linked() => {
                        return self.eval_t_params(mono_proj(fv.crack().clone(), rhs), level, t_loc)
                    }
                    Type::FreeVar(fv) if fv.is_unbound() => {
                        let (sub, sup) = fv.get_bound_types().unwrap();
                        (sub, Some(sup))
                    }
                    other => (other, None),
                };
                // cannot determine at this point
                if sub == Type::Never {
                    return Ok(mono_proj(*lhs, rhs));
                }
                for (_ty, ty_ctx) in self.get_nominal_super_type_ctxs(&sub).ok_or_else(|| {
                    EvalError::no_var_error(
                        self.cfg.input.clone(),
                        line!() as usize,
                        t_loc,
                        self.caused_by(),
                        &rhs,
                        None, // TODO:
                    )
                })? {
                    if let Ok(obj) = ty_ctx.get_const_local(&Token::symbol(&rhs), &self.name) {
                        if let ValueObj::Type(quant_t) = obj {
                            let subst_ctx = SubstContext::new(&sub, ty_ctx);
                            let t = subst_ctx.substitute(quant_t.typ().clone(), self, t_loc)?;
                            let t = self.eval_t_params(t, level, t_loc)?;
                            return Ok(t);
                        } else {
                            todo!()
                        }
                    }
                    for (class, methods) in ty_ctx.methods_list.iter() {
                        match (class, &opt_sup) {
                            (ClassDefType::ImplTrait { impl_trait, .. }, Some(sup)) => {
                                if !self.supertype_of(impl_trait, sup) {
                                    continue;
                                }
                            }
                            (ClassDefType::ImplTrait { impl_trait, .. }, None) => {
                                if !self.supertype_of(impl_trait, &sub) {
                                    continue;
                                }
                            }
                            _ => {}
                        }
                        if let Ok(obj) = methods.get_const_local(&Token::symbol(&rhs), &self.name) {
                            if let ValueObj::Type(quant_t) = obj {
                                let subst_ctx = SubstContext::new(&lhs, ty_ctx);
                                let t = subst_ctx.substitute(quant_t.typ().clone(), self, t_loc)?;
                                let t = self.eval_t_params(t, level, t_loc)?;
                                return Ok(t);
                            } else {
                                todo!()
                            }
                        }
                    }
                }
                let proj = mono_proj(*lhs, rhs);
                Err(EvalErrors::from(EvalError::no_candidate_error(
                    self.cfg.input.clone(),
                    line!() as usize,
                    &proj,
                    t_loc,
                    self.caused_by(),
                    self.get_no_candidate_hint(&proj),
                )))
            }
            Type::Ref(l) => Ok(ref_(self.eval_t_params(*l, level, t_loc)?)),
            Type::RefMut { before, after } => {
                let before = self.eval_t_params(*before, level, t_loc)?;
                let after = if let Some(after) = after {
                    Some(self.eval_t_params(*after, level, t_loc)?)
                } else {
                    None
                };
                Ok(ref_mut(before, after))
            }
            Type::BuiltinPoly { name, mut params } => {
                for p in params.iter_mut() {
                    *p = self.eval_tp(&mem::take(p))?;
                }
                Ok(builtin_poly(name, params))
            }
            Type::Poly {
                path,
                name,
                mut params,
            } => {
                for p in params.iter_mut() {
                    *p = self.eval_tp(&mem::take(p))?;
                }
                Ok(poly(path, name, params))
            }
            Type::And(l, r) => {
                let l = self.eval_t_params(*l, level, t_loc)?;
                let r = self.eval_t_params(*r, level, t_loc)?;
                Ok(and(l, r))
            }
            Type::Or(l, r) => {
                let l = self.eval_t_params(*l, level, t_loc)?;
                let r = self.eval_t_params(*r, level, t_loc)?;
                Ok(or(l, r))
            }
            Type::Not(l, r) => {
                let l = self.eval_t_params(*l, level, t_loc)?;
                let r = self.eval_t_params(*r, level, t_loc)?;
                Ok(not(l, r))
            }
            other if other.is_monomorphic() => Ok(other),
            _other => Err(EvalErrors::from(EvalError::feature_error(
                self.cfg.input.clone(),
                t_loc,
                "???",
                self.caused_by(),
            ))),
        }
    }

    pub(crate) fn _eval_bound(
        &self,
        bound: TyBound,
        level: usize,
        t_loc: Location,
    ) -> EvalResult<TyBound> {
        match bound {
            TyBound::Sandwiched { sub, mid, sup } => {
                let sub = self.eval_t_params(sub, level, t_loc)?;
                let mid = self.eval_t_params(mid, level, t_loc)?;
                let sup = self.eval_t_params(sup, level, t_loc)?;
                Ok(TyBound::sandwiched(sub, mid, sup))
            }
            TyBound::Instance { name: inst, t } => Ok(TyBound::instance(
                inst,
                self.eval_t_params(t, level, t_loc)?,
            )),
        }
    }

    pub(crate) fn eval_pred(&self, p: Predicate) -> EvalResult<Predicate> {
        match p {
            Predicate::Value(_) | Predicate::Const(_) => Ok(p),
            Predicate::Equal { lhs, rhs } => Ok(Predicate::eq(lhs, self.eval_tp(&rhs)?)),
            Predicate::NotEqual { lhs, rhs } => Ok(Predicate::ne(lhs, self.eval_tp(&rhs)?)),
            Predicate::LessEqual { lhs, rhs } => Ok(Predicate::le(lhs, self.eval_tp(&rhs)?)),
            Predicate::GreaterEqual { lhs, rhs } => Ok(Predicate::ge(lhs, self.eval_tp(&rhs)?)),
            Predicate::And(l, r) => Ok(Predicate::and(self.eval_pred(*l)?, self.eval_pred(*r)?)),
            Predicate::Or(l, r) => Ok(Predicate::or(self.eval_pred(*l)?, self.eval_pred(*r)?)),
            Predicate::Not(l, r) => Ok(Predicate::not(self.eval_pred(*l)?, self.eval_pred(*r)?)),
        }
    }

    pub(crate) fn get_tp_t(&self, p: &TyParam) -> EvalResult<Type> {
        let p = self.eval_tp(p)?;
        match p {
            TyParam::Value(ValueObj::Mut(v)) => Ok(v.borrow().class().mutate()),
            TyParam::Value(v) => Ok(v_enum(set![v])),
            TyParam::Erased(t) => Ok((*t).clone()),
            TyParam::FreeVar(fv) => {
                if let Some(t) = fv.get_type() {
                    Ok(t)
                } else {
                    todo!()
                }
            }
            // TODO: Class, Trait
            TyParam::Type(_) => Ok(Type::Type),
            TyParam::Mono(name) => self
                .rec_get_const_obj(&name)
                .map(|v| v_enum(set![v.clone()]))
                .ok_or_else(|| {
                    EvalErrors::from(EvalError::unreachable(
                        self.cfg.input.clone(),
                        fn_name!(),
                        line!(),
                    ))
                }),
            TyParam::MonoQVar(name) => {
                panic!("Not instantiated type variable: {name}")
            }
            TyParam::UnaryOp { op, val } => match op {
                OpKind::Mutate => Ok(self.get_tp_t(&val)?.mutate()),
                _ => todo!(),
            },
            other => todo!("{other}"),
        }
    }

    pub(crate) fn _get_tp_class(&self, p: &TyParam) -> EvalResult<Type> {
        let p = self.eval_tp(p)?;
        match p {
            TyParam::Value(v) => Ok(v.class()),
            TyParam::Erased(t) => Ok((*t).clone()),
            TyParam::FreeVar(fv) => {
                if let Some(t) = fv.get_type() {
                    Ok(t)
                } else {
                    todo!()
                }
            }
            TyParam::Type(_) => Ok(Type::Type),
            TyParam::Mono(name) => {
                self.rec_get_const_obj(&name)
                    .map(|v| v.class())
                    .ok_or_else(|| {
                        EvalErrors::from(EvalError::unreachable(
                            self.cfg.input.clone(),
                            fn_name!(),
                            line!(),
                        ))
                    })
            }
            other => todo!("{other}"),
        }
    }

    /// NOTE: lとrが型の場合はContextの方で判定する
    pub(crate) fn shallow_eq_tp(&self, lhs: &TyParam, rhs: &TyParam) -> bool {
        match (lhs, rhs) {
            (TyParam::Type(l), TyParam::Type(r)) => l == r,
            (TyParam::Value(l), TyParam::Value(r)) => l == r,
            (TyParam::Erased(l), TyParam::Erased(r)) => l == r,
            (TyParam::FreeVar { .. }, TyParam::FreeVar { .. }) => true,
            (TyParam::Mono(l), TyParam::Mono(r)) => {
                if l == r {
                    true
                } else if let (Some(l), Some(r)) =
                    (self.rec_get_const_obj(l), self.rec_get_const_obj(r))
                {
                    l == r
                } else {
                    // lとrが型の場合は...
                    false
                }
            }
            (TyParam::BinOp { .. }, TyParam::BinOp { .. }) => todo!(),
            (TyParam::UnaryOp { .. }, TyParam::UnaryOp { .. }) => todo!(),
            (TyParam::App { .. }, TyParam::App { .. }) => todo!(),
            (TyParam::Mono(m), TyParam::Value(l)) | (TyParam::Value(l), TyParam::Mono(m)) => {
                if let Some(o) = self.rec_get_const_obj(m) {
                    o == l
                } else {
                    true
                }
            }
            (TyParam::MonoQVar(_), _) | (_, TyParam::MonoQVar(_)) => false,
            (l, r) => todo!("l: {l}, r: {r}"),
        }
    }
}

use erg_common::config::ErgConfig;
use erg_common::traits::{ExitStatus, Runnable};
use erg_common::Str;

use crate::ast::AST;
use crate::desugar::Desugarer;
use crate::error::{ParserRunnerError, ParserRunnerErrors};
use crate::parse::ParserRunner;

/// Summarize parsing and desugaring
#[derive(Debug, Default)]
pub struct ASTBuilder {
    runner: ParserRunner,
}

impl Runnable for ASTBuilder {
    type Err = ParserRunnerError;
    type Errs = ParserRunnerErrors;
    const NAME: &'static str = "Erg AST builder";

    #[inline]
    fn new(cfg: ErgConfig) -> Self {
        Self {
            runner: ParserRunner::new(cfg),
        }
    }

    #[inline]
    fn cfg(&self) -> &ErgConfig {
        self.runner.cfg()
    }
    #[inline]
    fn cfg_mut(&mut self) -> &mut ErgConfig {
        self.runner.cfg_mut()
    }

    #[inline]
    fn finish(&mut self) {}

    #[inline]
    fn initialize(&mut self) {}

    #[inline]
    fn clear(&mut self) {}

    fn exec(&mut self) -> Result<ExitStatus, Self::Errs> {
        let src = self.cfg_mut().input.read();
        let ast = self.build(src)?;
        println!("{ast}");
        Ok(ExitStatus::OK)
    }

    fn eval(&mut self, src: String) -> Result<String, ParserRunnerErrors> {
        let ast = self.build(src)?;
        Ok(format!("{ast}"))
    }
}

impl ASTBuilder {
    pub fn build(&mut self, src: String) -> Result<AST, ParserRunnerErrors> {
        let module = self.runner.parse(src)?;
        let mut desugarer = Desugarer::new();
        let module = desugarer.desugar(module);
        let name = self.runner.cfg().input.unescaped_filename();
        let ast = AST::new(Str::rc(name), module);
        Ok(ast)
    }

    pub fn build_without_desugaring(&mut self, src: String) -> Result<AST, ParserRunnerErrors> {
        let module = self.runner.parse(src)?;
        let name = self.runner.cfg().input.unescaped_filename();
        let ast = AST::new(Str::rc(name), module);
        Ok(ast)
    }
}

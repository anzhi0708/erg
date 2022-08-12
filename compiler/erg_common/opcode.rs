//! defines `Opcode` (represents Python bytecode opcodes).
//!
//! Opcode(Pythonバイトコードオペコードを表す)を定義する

#![allow(dead_code)]
#![allow(non_camel_case_types)]

use crate::impl_display_from_debug;

/// Based on Python opcodes.
/// This is represented by u8.
///
/// TODO: implement all opcodes
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq)]
#[repr(u8)]
pub enum Opcode {
    POP_TOP = 1,
    ROT_TWO = 2,
    ROT_THREE = 3,
    DUP_TOP = 4,
    DUP_TOP2 = 5,
    ROT_FOUR = 6,
    NOP = 9,
    UNARY_POSITIVE = 10,
    UNARY_NEGATIVE = 11,
    UNARY_NOT = 12,
    UNARY_INVERT = 15,
    BINARY_MATRIX_MULTIPLY = 16,
    INPLACE_MATRIX_MULTIPLY = 17,
    BINARY_POWER = 19,
    BINARY_MULTIPLY = 20,
    BINARY_MODULO = 22,
    BINARY_ADD = 23,
    BINARY_SUBTRACT = 24,
    BINARY_SUBSCR = 25,
    BINARY_TRUE_DIVIDE = 27,
    INPLACE_FLOOR_DIVIDE = 28,
    INPLACE_TRUE_DIVIDE = 29,
    GET_LEN = 30,
    MATCH_MAPPING = 31,
    MATCH_SEQUENCE = 32,
    MATCH_KEYS = 33,
    PUSH_EXC_INFO = 35,
    CHECK_EXC_MATCH = 36,
    CHECK_EG_MATCH = 37,
    WITH_EXCEPT_START = 49,
    GET_AITER = 50,
    GET_ANEXT = 51,
    BEFORE_ASYNC_WITH = 52,
    BEFORE_WITH = 53,
    END_ASYNC_FOR = 54,
    // TODO:
    INPLACE_ADD = 55,
    INPLACE_SUBTRACT = 56,
    INPLACE_MULTIPLY = 57,
    INPLACE_MODULO = 59,
    STORE_SUBSCR = 60,
    BINARY_AND = 64,
    BINARY_XOR = 65,
    BINARY_OR = 66,
    GET_ITER = 68,
    GET_YIELD_FROM_ITER = 69,
    PRINT_EXPR = 70,
    LOAD_BUILD_CLASS = 71,
    LOAD_ASSERTION_ERROR = 74,
    RETURN_VALUE = 83,
    /* ↓ These opcodes take an arg */
    STORE_NAME = 90,
    DELETE_NAME = 91,
    UNPACK_SEQUENCE = 92,
    FOR_ITER = 93,
    UNPACK_EX = 94,
    STORE_ATTR = 95,
    STORE_GLOBAL = 97,
    LOAD_CONST = 100,
    LOAD_NAME = 101,
    BUILD_TUPLE = 102,
    BUILD_LIST = 103,
    BUILD_SET = 104,
    BUILD_MAP = 105, // build a Dict object
    LOAD_ATTR = 106,
    COMPARE_OP = 107,
    IMPORT_NAME = 108,
    IMPORT_FROM = 109,
    JUMP_FORWARD = 110,
    JUMP_IF_FALSE_OR_POP = 111,
    JUMP_IF_TRUE_OR_POP = 112,
    JUMP_ABSOLUTE = 113,
    POP_JUMP_IF_FALSE = 114,
    POP_JUMP_IF_TRUE = 115,
    LOAD_GLOBAL = 116,
    IS_OP = 117,
    CONTAINS_OP = 118,
    LOAD_FAST = 124,
    STORE_FAST = 125,
    DELETE_FAST = 126,
    RAISE_VARARGS = 130,
    CALL_FUNCTION = 131,
    MAKE_FUNCTION = 132,
    LOAD_CLOSURE = 135,
    LOAD_DEREF = 136,
    STORE_DEREF = 137,
    CALL_FUNCTION_KW = 141,
    LOAD_METHOD = 160,
    CALL_METHOD = 161,
    // Erg-specific opcodes (must have a unary `ERG_`)
    // Define in descending order from 219, 255
    ERG_POP_NTH = 196,
    ERG_PEEK_NTH = 197, // get ref to the arg-th element from TOS
    ERG_INC = 198,      // name += 1; arg: typecode
    ERG_DEC = 199,      // name -= 1
    ERG_LOAD_FAST_IMMUT = 200,
    ERG_STORE_FAST_IMMUT = 201,
    ERG_MOVE_FAST = 202,
    ERG_CLONE_FAST = 203,
    ERG_COPY_FAST = 204,
    ERG_REF_FAST = 205,
    ERG_REF_MUT_FAST = 206,
    ERG_MOVE_OUTER = 207,
    ERG_CLONE_OUTER = 208,
    ERG_COPY_OUTER = 209,
    ERG_REF_OUTER = 210,
    ERG_REF_MUT_OUTER = 211,
    ERG_LESS_THAN = 212,
    ERG_LESS_EQUAL = 213,
    ERG_EQUAL = 214,
    ERG_NOT_EQUAL = 215,
    ERG_MAKE_SLOT = 216,
    ERG_MAKE_TYPE = 217,
    ERG_MAKE_PURE_FUNCTION = 218,
    ERG_CALL_PURE_FUNCTION = 219,
    /* ↑ These opcodes take an arg ↑ */
    /* ↓ These opcodes take no arg ↓ */
    // ... = 220,
    ERG_LOAD_EMPTY_SLOT = 242,
    ERG_LOAD_EMPTY_STR = 243,
    ERG_LOAD_1_NAT = 244,
    ERG_LOAD_1_INT = 245,
    ERG_LOAD_1_REAL = 246,
    ERG_LOAD_NONE = 247,
    ERG_MUTATE = 248, // !x
    /// `[] =` (it doesn't cause any exceptions)
    ERG_STORE_SUBSCR = 249,
    // ... = 250,
    /// `= []` (it doesn't cause any exceptions)
    ERG_BINARY_SUBSCR = 251,
    ERG_BINARY_RANGE = 252,
    /// `/?` (rhs may be 0, it may cause a runtime panic)
    ERG_TRY_BINARY_DIVIDE = 253,
    /// `/` (rhs could not be 0, it doesn't cause any exceptions)
    ERG_BINARY_TRUE_DIVIDE = 254,
    NOT_IMPLEMENTED = 255,
}

use Opcode::*;

impl_display_from_debug!(Opcode);

impl From<u8> for Opcode {
    fn from(byte: u8) -> Self {
        match byte {
            1 => POP_TOP,
            2 => ROT_TWO,
            3 => ROT_THREE,
            4 => DUP_TOP,
            5 => DUP_TOP2,
            6 => ROT_FOUR,
            9 => NOP,
            10 => UNARY_POSITIVE,
            11 => UNARY_NEGATIVE,
            12 => UNARY_NOT,
            15 => UNARY_INVERT,
            19 => BINARY_POWER,
            20 => BINARY_MULTIPLY,
            22 => BINARY_MODULO,
            23 => BINARY_ADD,
            24 => BINARY_SUBTRACT,
            25 => BINARY_SUBSCR,
            27 => BINARY_TRUE_DIVIDE,
            28 => INPLACE_FLOOR_DIVIDE,
            29 => INPLACE_TRUE_DIVIDE,
            30 => GET_LEN,
            31 => MATCH_MAPPING,
            32 => MATCH_SEQUENCE,
            33 => MATCH_KEYS,
            35 => PUSH_EXC_INFO,
            36 => CHECK_EXC_MATCH,
            37 => CHECK_EG_MATCH,
            49 => WITH_EXCEPT_START,
            50 => GET_AITER,
            51 => GET_ANEXT,
            52 => BEFORE_ASYNC_WITH,
            53 => BEFORE_WITH,
            54 => END_ASYNC_FOR,
            55 => INPLACE_ADD,
            56 => INPLACE_SUBTRACT,
            57 => INPLACE_MULTIPLY,
            59 => INPLACE_MODULO,
            60 => STORE_SUBSCR,
            64 => BINARY_AND,
            65 => BINARY_XOR,
            66 => BINARY_OR,
            68 => GET_ITER,
            69 => GET_YIELD_FROM_ITER,
            70 => PRINT_EXPR,
            71 => LOAD_BUILD_CLASS,
            74 => LOAD_ASSERTION_ERROR,
            83 => RETURN_VALUE,
            /* ↓ These opcodes take an arg */
            90 => STORE_NAME,
            91 => DELETE_NAME,
            92 => UNPACK_SEQUENCE,
            93 => FOR_ITER,
            94 => UNPACK_EX,
            95 => STORE_ATTR,
            97 => STORE_GLOBAL,
            100 => LOAD_CONST,
            101 => LOAD_NAME,
            102 => BUILD_TUPLE,
            103 => BUILD_LIST,
            104 => BUILD_SET,
            105 => BUILD_MAP,
            106 => LOAD_ATTR,
            107 => COMPARE_OP,
            108 => IMPORT_NAME,
            109 => IMPORT_FROM,
            110 => JUMP_FORWARD,
            111 => JUMP_IF_FALSE_OR_POP,
            112 => JUMP_IF_TRUE_OR_POP,
            113 => JUMP_ABSOLUTE,
            114 => POP_JUMP_IF_FALSE,
            115 => POP_JUMP_IF_TRUE,
            116 => LOAD_GLOBAL,
            117 => IS_OP,
            118 => CONTAINS_OP,
            124 => LOAD_FAST,
            125 => STORE_FAST,
            126 => DELETE_FAST,
            130 => RAISE_VARARGS,
            131 => CALL_FUNCTION,
            132 => MAKE_FUNCTION,
            135 => LOAD_CLOSURE,
            136 => LOAD_DEREF,
            137 => STORE_DEREF,
            141 => CALL_FUNCTION_KW,
            160 => LOAD_METHOD,
            161 => CALL_METHOD,
            // Erg-specific opcodes
            196 => ERG_POP_NTH,
            197 => ERG_PEEK_NTH,
            198 => ERG_INC,
            199 => ERG_DEC,
            200 => ERG_LOAD_FAST_IMMUT,
            201 => ERG_STORE_FAST_IMMUT,
            202 => ERG_MOVE_FAST,
            203 => ERG_CLONE_FAST,
            204 => ERG_COPY_FAST,
            205 => ERG_REF_FAST,
            206 => ERG_REF_MUT_FAST,
            207 => ERG_MOVE_OUTER,
            208 => ERG_CLONE_OUTER,
            209 => ERG_COPY_OUTER,
            210 => ERG_REF_OUTER,
            211 => ERG_REF_MUT_OUTER,
            212 => ERG_LESS_THAN,
            213 => ERG_LESS_EQUAL,
            214 => ERG_EQUAL,
            215 => ERG_NOT_EQUAL,
            // ERG_GREATER_THAN is not necessary (can be done by inverting the argument of LESS_THAN)
            216 => ERG_MAKE_SLOT,
            217 => ERG_MAKE_TYPE,
            218 => ERG_MAKE_PURE_FUNCTION,
            219 => ERG_CALL_PURE_FUNCTION,
            /* ↑ These opcodes take an arg ↑ */
            /* ↓ These opcodes take no arg ↓ */
            // ... = 220,
            242 => ERG_LOAD_EMPTY_SLOT,
            243 => ERG_LOAD_EMPTY_STR,
            244 => ERG_LOAD_1_NAT,
            245 => ERG_LOAD_1_INT,
            246 => ERG_LOAD_1_REAL,
            247 => ERG_LOAD_NONE,
            248 => ERG_MUTATE,
            249 => ERG_STORE_SUBSCR,
            // 250 =>
            251 => ERG_BINARY_SUBSCR,
            252 => ERG_BINARY_RANGE,
            253 => ERG_TRY_BINARY_DIVIDE,
            254 => ERG_BINARY_TRUE_DIVIDE,
            255 => NOT_IMPLEMENTED,
            other => panic!("not implemented opcode: {other}"),
        }
    }
}

impl Opcode {
    pub const fn take_arg(&self) -> bool {
        90 <= (*self as u8) && (*self as u8) < 220
    }
}

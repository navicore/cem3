//! Runtime declarations for float arithmetic, comparisons, and
//! int/float/string conversions.

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[
    RuntimeDecl {
        decl: "declare ptr @patch_seq_push_float(ptr, double)",
        category: Some("; Float operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_add(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_subtract(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_multiply(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_divide(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_eq(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_lt(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_gt(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_lte(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_gte(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_neq(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_sqrt(ptr)",
        category: Some("; Float math"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_cbrt(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_pow(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_exp(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_ln(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_log10(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_log2(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_sin(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_cos(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_tan(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_asin(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_acos(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_atan(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_atan2(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_floor(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_ceil(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_round(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_trunc(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_pi(ptr)",
        category: Some("; Float constants"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_e(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_f_tau(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_int_to_float(ptr)",
        category: Some("; Float type conversions"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_float_to_int(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_float_to_string(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_to_float(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_int_to_bytes_i32_be(ptr)",
        category: Some("; Byte construction (binary protocol encoders)"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_float_to_bytes_f32_be(ptr)",
        category: None,
    },
];

pub(super) static SYMBOLS: &[(&str, &str)] = &[
    // Float arithmetic
    ("f.add", "patch_seq_f_add"),
    ("f.subtract", "patch_seq_f_subtract"),
    ("f.multiply", "patch_seq_f_multiply"),
    ("f.divide", "patch_seq_f_divide"),
    // Terse float arithmetic aliases
    ("f.+", "patch_seq_f_add"),
    ("f.-", "patch_seq_f_subtract"),
    ("f.*", "patch_seq_f_multiply"),
    ("f./", "patch_seq_f_divide"),
    // Float comparison (symbol form)
    ("f.=", "patch_seq_f_eq"),
    ("f.<", "patch_seq_f_lt"),
    ("f.>", "patch_seq_f_gt"),
    ("f.<=", "patch_seq_f_lte"),
    ("f.>=", "patch_seq_f_gte"),
    ("f.<>", "patch_seq_f_neq"),
    // Float comparison (verbose form)
    ("f.eq", "patch_seq_f_eq"),
    ("f.lt", "patch_seq_f_lt"),
    ("f.gt", "patch_seq_f_gt"),
    ("f.lte", "patch_seq_f_lte"),
    ("f.gte", "patch_seq_f_gte"),
    ("f.neq", "patch_seq_f_neq"),
    // Float math
    ("f.sqrt", "patch_seq_f_sqrt"),
    ("f.cbrt", "patch_seq_f_cbrt"),
    ("f.pow", "patch_seq_f_pow"),
    ("f.exp", "patch_seq_f_exp"),
    ("f.ln", "patch_seq_f_ln"),
    ("f.log10", "patch_seq_f_log10"),
    ("f.log2", "patch_seq_f_log2"),
    ("f.sin", "patch_seq_f_sin"),
    ("f.cos", "patch_seq_f_cos"),
    ("f.tan", "patch_seq_f_tan"),
    ("f.asin", "patch_seq_f_asin"),
    ("f.acos", "patch_seq_f_acos"),
    ("f.atan", "patch_seq_f_atan"),
    ("f.atan2", "patch_seq_f_atan2"),
    ("f.floor", "patch_seq_f_floor"),
    ("f.ceil", "patch_seq_f_ceil"),
    ("f.round", "patch_seq_f_round"),
    ("f.trunc", "patch_seq_f_trunc"),
    // Float constants
    ("f.pi", "patch_seq_f_pi"),
    ("f.e", "patch_seq_f_e"),
    ("f.tau", "patch_seq_f_tau"),
    // Float type conversions
    ("int->float", "patch_seq_int_to_float"),
    ("float->int", "patch_seq_float_to_int"),
    ("float->string", "patch_seq_float_to_string"),
    ("string->float", "patch_seq_string_to_float"),
    // Byte construction (binary protocol encoders)
    ("int.to-bytes-i32-be", "patch_seq_int_to_bytes_i32_be"),
    ("float.to-bytes-f32-be", "patch_seq_float_to_bytes_f32_be"),
];

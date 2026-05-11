//! Integer arithmetic and comparison, boolean, bitwise, and LLVM intrinsic
//! declarations. These cover the primitive operations whose runtime entry
//! points are used by codegen both for FFI and fallback paths.

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[
    // Integer arithmetic
    RuntimeDecl {
        decl: "declare ptr @patch_seq_add(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_subtract(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_multiply(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_divide(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_modulo(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_pow(ptr)",
        category: None,
    },
    // Integer comparisons
    RuntimeDecl {
        decl: "declare ptr @patch_seq_eq(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_lt(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_gt(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_lte(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_gte(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_neq(ptr)",
        category: None,
    },
    // Boolean operations
    RuntimeDecl {
        decl: "declare ptr @patch_seq_and(ptr)",
        category: Some("; Boolean operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_or(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_not(ptr)",
        category: None,
    },
    // Bitwise operations
    RuntimeDecl {
        decl: "declare ptr @patch_seq_band(ptr)",
        category: Some("; Bitwise operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_bor(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_bxor(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_bnot(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_shl(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_shr(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_popcount(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_clz(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_ctz(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_int_bits(ptr)",
        category: None,
    },
    // LLVM intrinsics
    RuntimeDecl {
        decl: "declare i64 @llvm.ctpop.i64(i64)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare i64 @llvm.ctlz.i64(i64, i1)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare i64 @llvm.cttz.i64(i64, i1)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare void @llvm.trap() noreturn nounwind",
        category: None,
    },
];

pub(super) static SYMBOLS: &[(&str, &str)] = &[
    // Integer arithmetic
    ("i.add", "patch_seq_add"),
    ("i.subtract", "patch_seq_subtract"),
    ("i.multiply", "patch_seq_multiply"),
    ("i.divide", "patch_seq_divide"),
    ("i.modulo", "patch_seq_modulo"),
    ("i.pow", "patch_seq_pow"),
    // Terse integer arithmetic aliases
    ("i.+", "patch_seq_add"),
    ("i.-", "patch_seq_subtract"),
    ("i.*", "patch_seq_multiply"),
    ("i./", "patch_seq_divide"),
    ("i.%", "patch_seq_modulo"),
    // Integer comparison (symbol form)
    ("i.=", "patch_seq_eq"),
    ("i.<", "patch_seq_lt"),
    ("i.>", "patch_seq_gt"),
    ("i.<=", "patch_seq_lte"),
    ("i.>=", "patch_seq_gte"),
    ("i.<>", "patch_seq_neq"),
    // Integer comparison (verbose form)
    ("i.eq", "patch_seq_eq"),
    ("i.lt", "patch_seq_lt"),
    ("i.gt", "patch_seq_gt"),
    ("i.lte", "patch_seq_lte"),
    ("i.gte", "patch_seq_gte"),
    ("i.neq", "patch_seq_neq"),
    // Boolean
    ("and", "patch_seq_and"),
    ("or", "patch_seq_or"),
    ("not", "patch_seq_not"),
    // Bitwise
    ("band", "patch_seq_band"),
    ("bor", "patch_seq_bor"),
    ("bxor", "patch_seq_bxor"),
    ("bnot", "patch_seq_bnot"),
    ("shl", "patch_seq_shl"),
    ("shr", "patch_seq_shr"),
    ("popcount", "patch_seq_popcount"),
    ("clz", "patch_seq_clz"),
    ("ctz", "patch_seq_ctz"),
    ("int-bits", "patch_seq_int_bits"),
];

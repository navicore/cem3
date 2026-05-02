use super::*;
use crate::types::{StackType, Type};

#[test]
fn test_builtin_signature_write_line() {
    let sig = builtin_signature("io.write-line").unwrap();
    // ( ..a String -- ..a )
    let (rest, top) = sig.inputs.clone().pop().unwrap();
    assert_eq!(top, Type::String);
    assert_eq!(rest, StackType::RowVar("a".to_string()));
    assert_eq!(sig.outputs, StackType::RowVar("a".to_string()));
}

#[test]
fn test_builtin_signature_i_add() {
    let sig = builtin_signature("i.add").unwrap();
    // ( ..a Int Int -- ..a Int )
    let (rest, top) = sig.inputs.clone().pop().unwrap();
    assert_eq!(top, Type::Int);
    let (rest2, top2) = rest.pop().unwrap();
    assert_eq!(top2, Type::Int);
    assert_eq!(rest2, StackType::RowVar("a".to_string()));

    let (rest3, top3) = sig.outputs.clone().pop().unwrap();
    assert_eq!(top3, Type::Int);
    assert_eq!(rest3, StackType::RowVar("a".to_string()));
}

#[test]
fn test_builtin_signature_dup() {
    let sig = builtin_signature("dup").unwrap();
    // Input: ( ..a T )
    assert_eq!(
        sig.inputs,
        StackType::Cons {
            rest: Box::new(StackType::RowVar("a".to_string())),
            top: Type::Var("T".to_string())
        }
    );
    // Output: ( ..a T T )
    let (rest, top) = sig.outputs.clone().pop().unwrap();
    assert_eq!(top, Type::Var("T".to_string()));
    let (rest2, top2) = rest.pop().unwrap();
    assert_eq!(top2, Type::Var("T".to_string()));
    assert_eq!(rest2, StackType::RowVar("a".to_string()));
}

#[test]
fn test_all_builtins_have_signatures() {
    let sigs = builtin_signatures();

    // Verify all expected builtins have signatures
    assert!(sigs.contains_key("io.write-line"));
    assert!(sigs.contains_key("io.read-line"));
    assert!(sigs.contains_key("int->string"));
    assert!(sigs.contains_key("i.add"));
    assert!(sigs.contains_key("dup"));
    assert!(sigs.contains_key("swap"));
    assert!(sigs.contains_key("chan.make"));
    assert!(sigs.contains_key("chan.send"));
    assert!(sigs.contains_key("chan.receive"));
    assert!(
        sigs.contains_key("string->float"),
        "string->float should be a builtin"
    );
    assert!(
        sigs.contains_key("signal.trap"),
        "signal.trap should be a builtin"
    );
}

#[test]
fn test_all_docs_have_signatures() {
    let sigs = builtin_signatures();
    let docs = builtin_docs();

    for name in docs.keys() {
        assert!(
            sigs.contains_key(*name),
            "Builtin '{}' has documentation but no signature",
            name
        );
    }
}

#[test]
fn test_all_signatures_have_docs() {
    let sigs = builtin_signatures();
    let docs = builtin_docs();

    for name in sigs.keys() {
        assert!(
            docs.contains_key(name.as_str()),
            "Builtin '{}' has signature but no documentation",
            name
        );
    }
}

#[test]
fn test_categories_cover_every_signature() {
    // Guards against adding a new builtin sub-module without registering
    // it in `builtin_categories()` — which would silently drop those words
    // from the LSP `seq/listWords` response and any quick-reference UI.
    let sigs = builtin_signatures();
    let mut categorised = std::collections::HashSet::new();
    for (_, words) in builtin_categories() {
        for w in words {
            categorised.insert(w);
        }
    }

    for name in sigs.keys() {
        assert!(
            categorised.contains(name),
            "Builtin '{}' has a signature but is not enumerated by builtin_categories() — \
             add its sub-module to the list in builtins.rs",
            name
        );
    }
    assert_eq!(
        categorised.len(),
        sigs.len(),
        "builtin_categories() listed extra words not in builtin_signatures()"
    );
}

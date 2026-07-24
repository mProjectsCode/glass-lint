use smol_str::SmolStr;

use crate::name::NameId;

use super::*;

#[test]
fn name_path_empty() {
    let path = NamePath::new();
    assert!(path.is_root());
    assert!(path.segments().is_empty());
    assert_eq!(path.first_segment(), None);
    assert_eq!(path.last_segment(), None);
}

#[test]
fn name_path_single_segment() {
    let id = NameId(42);
    let mut path = NamePath::new();
    path.append(id);
    assert_eq!(path.segments(), &[NameId(42)]);
    assert_eq!(path.first_segment(), Some(&NameId(42)));
    assert_eq!(path.last_segment(), Some(&NameId(42)));
    assert!(path.is_root());
}

#[test]
fn name_path_multi_segment() {
    let mut path = NamePath::new();
    path.append(NameId(1));
    path.append(NameId(2));
    path.append(NameId(3));
    assert_eq!(path.segments(), &[NameId(1), NameId(2), NameId(3)]);
    assert!(!path.is_root());
}

#[test]
fn name_path_without_first() {
    let mut path = NamePath::new();
    path.append(NameId(1));
    path.append(NameId(2));
    let rest = path.without_first_segment().unwrap();
    assert_eq!(rest.segments(), &[NameId(2)]);
}

#[test]
fn name_path_without_first_on_single_returns_empty() {
    let mut path = NamePath::new();
    path.append(NameId(1));
    let rest = path.without_first_segment().unwrap();
    assert!(rest.is_root());
    assert!(rest.segments().is_empty());
}

#[test]
fn name_path_without_first_on_empty_returns_none() {
    let path = NamePath::new();
    assert_eq!(path.without_first_segment(), None);
}

#[test]
fn name_path_without_last() {
    let mut path = NamePath::new();
    path.append(NameId(1));
    path.append(NameId(2));
    let rest = path.without_last_segment().unwrap();
    assert_eq!(rest.segments(), &[NameId(1)]);
}

#[test]
fn name_path_without_last_on_empty_returns_none() {
    let path = NamePath::new();
    assert_eq!(path.without_last_segment(), None);
}

#[test]
fn name_path_append_path() {
    let mut a = NamePath::new();
    a.append(NameId(1));
    let mut b = NamePath::new();
    b.append(NameId(2));
    b.append(NameId(3));
    let c = a.append_path(&b);
    assert_eq!(c.segments(), &[NameId(1), NameId(2), NameId(3)]);
}

#[test]
fn name_path_is_root() {
    assert!(NamePath::new().is_root());
    let mut p = NamePath::new();
    p.append(NameId(1));
    assert!(p.is_root());
    p.append(NameId(2));
    assert!(!p.is_root());
}

#[test]
fn name_path_from_ids() {
    let ids = [NameId(10), NameId(20)];
    let path = NamePath::from_ids(ids);
    assert_eq!(path.segments(), &[NameId(10), NameId(20)]);
}

#[test]
fn name_path_is_equal_or_descendant_of() {
    let mut root = NamePath::new();
    root.append(NameId(1));
    let mut child = NamePath::new();
    child.append(NameId(1));
    child.append(NameId(2));
    assert!(child.is_equal_or_descendant_of(&root));
    assert!(root.is_equal_or_descendant_of(&root));
    assert!(!root.is_equal_or_descendant_of(&child));
}

#[test]
fn symbol_path_from_chain_with_dots() {
    let path = SymbolPath::from_chain("a.b.c");
    assert_eq!(
        path.segments(),
        &[SmolStr::new("a"), SmolStr::new("b"), SmolStr::new("c")]
    );
}

#[test]
fn symbol_path_without_first() {
    let path = SymbolPath::from_chain("a.b.c");
    let rest = path.without_first_segment().unwrap();
    assert_eq!(rest.segments(), &[SmolStr::new("b"), SmolStr::new("c")]);
}

#[test]
fn symbol_path_without_last() {
    let path = SymbolPath::from_chain("a.b.c");
    let rest = path.without_last_segment().unwrap();
    assert_eq!(rest.segments(), &[SmolStr::new("a"), SmolStr::new("b")]);
}

#[test]
fn symbol_path_from_impls() {
    let from_str: SymbolPath = "a.b.c".into();
    let from_string: SymbolPath = String::from("a.b.c").into();
    let from_smol: SymbolPath = SmolStr::new("a.b.c").into();
    assert_eq!(from_str, from_string);
    assert_eq!(from_str, from_smol);
}

#[test]
fn symbol_path_is_root() {
    assert!(SymbolPath::from_chain("a").is_root());
    assert!(!SymbolPath::from_chain("a.b").is_root());
}

#[test]
fn symbol_path_is_equal_or_descendant_of() {
    let root = SymbolPath::from_chain("a.b");
    let child = SymbolPath::from_chain("a.b.c");
    assert!(child.is_equal_or_descendant_of(&root));
    assert!(root.is_equal_or_descendant_of(&root));
    assert!(!root.is_equal_or_descendant_of(&child));
}

#[test]
fn symbol_path_edge_cases() {
    assert!(SymbolPath::from_chain("").is_empty());
    assert!(SymbolPath::from_chain(".").is_empty());
    assert!(SymbolPath::from_chain("..").is_empty());
    assert_eq!(SymbolPath::from_chain(".a."), SymbolPath::from_chain("a"));
}

#[test]
fn symbol_path_from_chain_strips_leading_trailing_consecutive_dots() {
    let path = SymbolPath::from_chain(".a..b.");
    assert_eq!(path.segments(), &[SmolStr::new("a"), SmolStr::new("b")]);
}

#[test]
fn symbol_path_append_path() {
    let a = SymbolPath::from_chain("a.b");
    let b = SymbolPath::from_chain("c.d");
    let c = a.append_path(&b);
    assert_eq!(
        c.segments(),
        &[
            SmolStr::new("a"),
            SmolStr::new("b"),
            SmolStr::new("c"),
            SmolStr::new("d"),
        ]
    );
}

#[test]
fn symbol_path_append_empty() {
    let a = SymbolPath::from_chain("a");
    let empty = SymbolPath::from_chain("");
    let c = a.append_path(&empty);
    assert_eq!(c.segments(), &[SmolStr::new("a")]);
}

#[test]
fn symbol_path_first_segment_empty() {
    let path = SymbolPath::from_chain("");
    assert_eq!(path.first_segment(), None);
}

#[test]
fn symbol_path_is_equal_or_descendant_of_not_ancestor() {
    let a = SymbolPath::from_chain("a.b");
    let b = SymbolPath::from_chain("a.c");
    assert!(!a.is_equal_or_descendant_of(&b));
    assert!(!b.is_equal_or_descendant_of(&a));
}

#[test]
fn name_path_len() {
    let mut path = NamePath::new();
    assert_eq!(path.len(), 0);
    path.append(NameId(1));
    assert_eq!(path.len(), 1);
    path.append(NameId(2));
    assert_eq!(path.len(), 2);
}

#[test]
fn name_path_is_empty() {
    assert!(NamePath::new().is_empty());
    let mut path = NamePath::new();
    path.append(NameId(1));
    assert!(!path.is_empty());
}

#[test]
fn name_path_append_path_empty() {
    let mut a = NamePath::new();
    a.append(NameId(1));
    let empty = NamePath::new();
    let c = a.append_path(&empty);
    assert_eq!(c.segments(), &[NameId(1)]);
}

#[test]
fn path_view_empty() {
    let view = PathView::<i32>::new(&[]);
    assert!(view.is_empty());
    assert!(view.is_root());
    assert_eq!(view.first_segment(), None);
    assert_eq!(view.last_segment(), None);
    assert_eq!(view.len(), 0);
}

#[test]
fn path_view_single() {
    let view = PathView::new(&[42]);
    assert!(!view.is_empty());
    assert!(view.is_root());
    assert_eq!(view.first_segment(), Some(&42));
    assert_eq!(view.last_segment(), Some(&42));
}

#[test]
fn path_view_multi() {
    let view = PathView::new(&[1, 2, 3]);
    assert_eq!(view.segments(), &[1, 2, 3]);
    assert!(!view.is_root());
    assert_eq!(view.first_segment(), Some(&1));
    assert_eq!(view.last_segment(), Some(&3));
}

#[test]
fn path_view_without_last() {
    let view = PathView::new(&[1, 2, 3]);
    let rest = view.without_last_segment().unwrap();
    assert_eq!(rest.segments(), &[1, 2]);
}

#[test]
fn path_view_without_last_on_empty_returns_none() {
    let view = PathView::<i32>::new(&[]);
    assert_eq!(view.without_last_segment(), None);
}

#[test]
fn path_view_without_first() {
    let view = PathView::new(&[1, 2, 3]);
    let rest = view.without_first_segment().unwrap();
    assert_eq!(rest.segments(), &[2, 3]);
}

#[test]
fn path_view_without_first_on_empty_returns_none() {
    let view = PathView::<i32>::new(&[]);
    assert_eq!(view.without_first_segment(), None);
}

#[test]
fn path_view_is_equal_or_descendant_of() {
    let root = PathView::new(&[1, 2]);
    let child = PathView::new(&[1, 2, 3]);
    assert!(child.is_equal_or_descendant_of(&root));
    assert!(root.is_equal_or_descendant_of(&root));
    assert!(!root.is_equal_or_descendant_of(&child));
}

#[test]
fn path_as_view_on_name_path() {
    let mut path = NamePath::new();
    path.append(NameId(1));
    path.append(NameId(2));
    let view = path.as_view();
    assert_eq!(view.segments(), &[NameId(1), NameId(2)]);
    assert_eq!(view.first_segment(), Some(&NameId(1)));
    assert_eq!(view.len(), 2);
}

#[test]
fn path_as_view_on_symbol_path() {
    let path = SymbolPath::from_chain("a.b");
    let view = path.as_view();
    assert_eq!(view.segments(), &[SmolStr::new("a"), SmolStr::new("b")]);
    assert_eq!(view.first_segment(), Some(&SmolStr::new("a")));
}

#[test]
fn view_without_last_segment_on_name_path() {
    let mut path = NamePath::new();
    path.append(NameId(1));
    path.append(NameId(2));
    path.append(NameId(3));
    let view = path.view_without_last_segment().unwrap();
    assert_eq!(view.segments(), &[NameId(1), NameId(2)]);
}

#[test]
fn view_without_last_segment_on_symbol_path() {
    let path = SymbolPath::from_chain("a.b.c");
    let view = path.view_without_last_segment().unwrap();
    assert_eq!(view.segments(), &[SmolStr::new("a"), SmolStr::new("b")]);
}

#[test]
fn view_without_last_segment_on_empty() {
    let path = NamePath::new();
    assert_eq!(path.view_without_last_segment(), None);
}

#[test]
fn view_without_first_segment_on_name_path() {
    let mut path = NamePath::new();
    path.append(NameId(1));
    path.append(NameId(2));
    let view = path.view_without_first_segment().unwrap();
    assert_eq!(view.segments(), &[NameId(2)]);
}

#[test]
fn view_without_first_segment_on_symbol_path() {
    let path = SymbolPath::from_chain("a.b.c");
    let view = path.view_without_first_segment().unwrap();
    assert_eq!(view.segments(), &[SmolStr::new("b"), SmolStr::new("c")]);
}

#[test]
fn view_without_first_segment_on_empty() {
    let path = NamePath::new();
    assert_eq!(path.view_without_first_segment(), None);
}

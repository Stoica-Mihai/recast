#![allow(clippy::unwrap_used)]

use super::*;

fn map(pairs: &[(&str, &str)]) -> RenameMap {
    RenameMap::new(pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()).unwrap()
}

#[test]
fn dependent_renames_do_not_collapse_into_each_other() {
    let m = map(&[("Foo", "Bar"), ("Bar", "Baz")]);
    let out = m.rewrite("use Foo;\nuse Bar;\nfn f(a: Foo, b: Bar) {}\n");
    assert_eq!(out.after, "use Bar;\nuse Baz;\nfn f(a: Bar, b: Baz) {}\n");
    assert_eq!(out.matches, 4);
}

#[test]
fn a_swap_exchanges_both_names() {
    let m = map(&[("Foo", "Bar"), ("Bar", "Foo")]);
    let out = m.rewrite("Foo Bar Foo\n");
    assert_eq!(out.after, "Bar Foo Bar\n");
}

#[test]
fn a_swap_is_accepted_but_is_not_rerunnable() {
    assert!(!map(&[("Foo", "Bar"), ("Bar", "Foo")]).rerunnable());
}

#[test]
fn a_chain_is_accepted_but_is_not_rerunnable() {
    assert!(!map(&[("Foo", "Bar"), ("Bar", "Baz")]).rerunnable());
}

#[test]
fn a_map_whose_replacements_share_no_name_with_its_keys_is_rerunnable() {
    assert!(map(&[("Foo", "Qux"), ("Bar", "Quux")]).rerunnable());
}

#[test]
fn a_map_that_expands_a_name_into_itself_is_refused() {
    let err = RenameMap::new(vec![("Foo".to_owned(), "Foo Bar".to_owned())]).unwrap_err();
    assert!(matches!(err, Error::RenameMapDiverges { .. }), "{err:?}");
}

#[test]
fn a_doubling_map_is_refused() {
    let err = RenameMap::new(vec![("Foo".to_owned(), "Foo Foo".to_owned())]).unwrap_err();
    assert!(matches!(err, Error::RenameMapDiverges { .. }), "{err:?}");
}

#[test]
fn an_expanding_cycle_is_refused() {
    let err = RenameMap::new(vec![
        ("Foo".to_owned(), "Bar".to_owned()),
        ("Bar".to_owned(), "Foo Foo".to_owned()),
    ])
    .unwrap_err();
    assert!(matches!(err, Error::RenameMapDiverges { .. }), "{err:?}");
}

#[test]
fn keys_match_whole_words_only() {
    let m = map(&[("Foo", "X")]);
    let out = m.rewrite("Foo Foobar barFoo Foo\n");
    assert_eq!(out.after, "X Foobar barFoo X\n");
    assert_eq!(out.matches, 2);
}

#[test]
fn a_longer_key_wins_over_a_shorter_one_sharing_its_prefix() {
    let out = map(&[("Foo", "A"), ("Foo-Bar", "B")]).rewrite("Foo-Bar and Foo\n");
    assert_eq!(out.after, "B and A\n");
}

#[test]
fn key_order_does_not_change_the_result() {
    let forward = map(&[("Foo", "A"), ("Foo-Bar", "B")]).rewrite("Foo-Bar\n").after;
    let reversed = map(&[("Foo-Bar", "B"), ("Foo", "A")]).rewrite("Foo-Bar\n").after;
    assert_eq!(forward, reversed);
    assert_eq!(forward, "B\n");
}

#[test]
fn a_key_is_matched_literally_not_as_a_regex() {
    let out = map(&[("a.b", "X")]).rewrite("a.b axb\n");
    assert_eq!(out.after, "X axb\n");
}

#[test]
fn renaming_to_an_empty_string_deletes_the_name() {
    let out = map(&[("Foo", "")]).rewrite("Foo bar\n");
    assert_eq!(out.after, " bar\n");
}

#[test]
fn an_identity_rename_is_a_rerunnable_noop() {
    let m = map(&[("Foo", "Foo")]);
    assert!(m.rerunnable());
    assert_eq!(m.rewrite("Foo\n").after, "Foo\n");
}

#[test]
fn an_empty_map_is_refused() {
    let err = RenameMap::new(vec![]).unwrap_err();
    assert!(matches!(err, Error::InvalidRenameMap { .. }), "{err:?}");
}

#[test]
fn an_empty_key_is_refused() {
    let err = RenameMap::new(vec![(String::new(), "X".to_owned())]).unwrap_err();
    assert!(matches!(err, Error::InvalidRenameMap { .. }), "{err:?}");
}

#[test]
fn a_duplicate_key_is_refused() {
    let err = RenameMap::new(vec![
        ("Foo".to_owned(), "A".to_owned()),
        ("Foo".to_owned(), "B".to_owned()),
    ])
    .unwrap_err();
    assert!(matches!(err, Error::InvalidRenameMap { .. }), "{err:?}");
}

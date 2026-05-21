#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn build_pool_with_explicit_thread_count() {
    let pool = build_pool(Some(2)).unwrap();
    assert_eq!(pool.current_num_threads(), 2);
}

#[test]
fn build_pool_with_one_thread() {
    let pool = build_pool(Some(1)).unwrap();
    assert_eq!(pool.current_num_threads(), 1);
}

#[test]
fn build_pool_default_is_at_least_one() {
    let pool = build_pool(None).unwrap();
    assert!(pool.current_num_threads() >= 1);
}

#[test]
fn build_pool_zero_threads_is_rejected() {
    assert!(build_pool(Some(0)).is_err());
}

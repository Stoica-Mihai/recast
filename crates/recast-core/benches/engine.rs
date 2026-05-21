//! Criterion benchmarks for the recast engine.
//!
//! Run with `cargo bench --features lang-rust,script`. HTML reports
//! land under `target/criterion/`.

#![allow(clippy::unwrap_used)]

use std::fs;

use criterion::{Criterion, criterion_group, criterion_main};
use recast_core::{
    CompiledPattern, Language, PatternOptions, PlanOptions, plan_rewrite, plan_structural_rewrite,
    structural_rewrite,
};
use tempfile::TempDir;

fn bench_pattern_compile(c: &mut Criterion) {
    c.bench_function("pattern_compile_simple", |b| {
        b.iter(|| {
            let _ =
                CompiledPattern::compile("OldName", "NewName", &PatternOptions::default()).unwrap();
        });
    });

    c.bench_function("pattern_compile_complex", |b| {
        b.iter(|| {
            let _ = CompiledPattern::compile(
                r"fn (\w+)\s*\(([^)]*)\)\s*->\s*(\w+)",
                "fn ${1}_v2($2) -> ${3}",
                &PatternOptions::default(),
            )
            .unwrap();
        });
    });
}

fn fixture(file_count: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    for i in 0..file_count {
        let path = dir.path().join(format!("f{i}.rs"));
        fs::write(&path, format!("fn OldName_{i}() {{ OldName_{i}(); }}\n")).unwrap();
    }
    dir
}

fn bench_plan_rewrite(c: &mut Criterion) {
    let mut group = c.benchmark_group("plan_rewrite");
    for size in [10usize, 100, 500] {
        let dir = fixture(size);
        let root = dir.path().to_path_buf();
        group.bench_function(format!("{size}_files"), |b| {
            b.iter(|| {
                let plan = plan_rewrite(
                    "OldName",
                    "NewName",
                    &[&root],
                    &PlanOptions { at_least: Some(0), ..PlanOptions::default() },
                )
                .unwrap();
                std::hint::black_box(plan);
            });
        });
    }
    group.finish();
}

fn bench_structural_rewrite(c: &mut Criterion) {
    let source =
        (0..200).map(|i| format!("fn fn_{i}() {{ fn_{i}(); }}")).collect::<Vec<_>>().join("\n");
    c.bench_function("structural_rewrite_rename_one_identifier", |b| {
        b.iter(|| {
            let out = structural_rewrite(
                Language::Rust,
                &source,
                r#"((identifier) @id (#eq? @id "fn_42"))"#,
                "renamed",
            )
            .unwrap();
            std::hint::black_box(out);
        });
    });
}

fn bench_plan_structural_rewrite(c: &mut Criterion) {
    let mut group = c.benchmark_group("plan_structural_rewrite");
    for size in [10usize, 100, 500] {
        let dir = fixture(size);
        let root = dir.path().to_path_buf();
        group.bench_function(format!("{size}_files"), |b| {
            b.iter(|| {
                let plan = plan_structural_rewrite(
                    Language::Rust,
                    r#"(identifier) @id"#,
                    "X",
                    &[&root],
                    &PlanOptions { at_least: Some(0), ..PlanOptions::default() },
                )
                .unwrap();
                std::hint::black_box(plan);
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_pattern_compile,
    bench_plan_rewrite,
    bench_structural_rewrite,
    bench_plan_structural_rewrite,
);
criterion_main!(benches);

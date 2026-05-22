window.BENCHMARK_DATA = {
  "lastUpdate": 1779459023685,
  "repoUrl": "https://github.com/Stoica-Mihai/recast",
  "entries": {
    "recast-core engine benches": [
      {
        "commit": {
          "author": {
            "email": "Stoica-Mihai@users.noreply.github.com",
            "name": "MCS",
            "username": "Stoica-Mihai"
          },
          "committer": {
            "email": "Stoica-Mihai@users.noreply.github.com",
            "name": "MCS",
            "username": "Stoica-Mihai"
          },
          "distinct": true,
          "id": "7527efb11e98dd06ed8f97ee909cb844f93c544b",
          "message": "ci: criterion regression gate via github-action-benchmark\n\nAdds .github/workflows/bench.yml so the criterion suite in\ncrates/recast-core/benches/engine.rs runs on every push to main and\nevery PR against main:\n\n- Bench output piped through criterion's --output-format=bencher so\n  each measurement comes out as a single\n  `test … bench: N ns/iter (+/- M)` line that\n  benchmark-action/github-action-benchmark parses natively.\n- On push to main: stores the run as the new baseline on the\n  gh-pages branch (auto-created).\n- On pull_request: compares against the latest baseline, comments\n  on the PR if any bench slows by more than 50%, and fails the job\n  so the regression is visible in the required-checks list.\n\nThreshold is intentionally loose (150%) at first; tighten as a\nstable baseline accumulates over a couple of weeks. The workflow\ninherits the FORCE_JAVASCRIPT_ACTIONS_TO_NODE24 env so it doesn't\nre-introduce the Node 20 deprecation noise.",
          "timestamp": "2026-05-22T16:58:10+03:00",
          "tree_id": "434a032728275c2b258a9eb5508e87bfc3646e33",
          "url": "https://github.com/Stoica-Mihai/recast/commit/7527efb11e98dd06ed8f97ee909cb844f93c544b"
        },
        "date": 1779459023196,
        "tool": "cargo",
        "benches": [
          {
            "name": "pattern_compile_simple",
            "value": 2301,
            "range": "± 98",
            "unit": "ns/iter"
          },
          {
            "name": "pattern_compile_complex",
            "value": 625622,
            "range": "± 3162",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/10_files",
            "value": 1767635,
            "range": "± 153881",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/100_files",
            "value": 2404290,
            "range": "± 95779",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/500_files",
            "value": 3686039,
            "range": "± 141139",
            "unit": "ns/iter"
          },
          {
            "name": "structural_rewrite_rename_one_identifier",
            "value": 3810443,
            "range": "± 30148",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/10_files",
            "value": 3494321,
            "range": "± 145932",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/100_files",
            "value": 4549355,
            "range": "± 171040",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/500_files",
            "value": 7408378,
            "range": "± 192226",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}
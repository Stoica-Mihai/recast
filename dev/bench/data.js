window.BENCHMARK_DATA = {
  "lastUpdate": 1779539491463,
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
      },
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
          "id": "5598fafc2ae4df548843204bb0758161b22e6109",
          "message": "docs: README \"For AI agents\" section + CHANGELOG entry for recast-mcp\n\nREADME gains a top-level \"For AI agents (MCP server)\" section that\nexplains the install path, the Claude Desktop config snippet, the\nfour tools, and the safety pitch (\"why agents reach for it instead\nof write_file loops or sed\"). Test-count headline bumped 130 → 136\nto reflect the symlink + concurrent-apply regression tests added in\nrecent commits.\n\nCHANGELOG [Unreleased] section accumulates everything since 0.1.8:\nthe MCP server, walker symlink tests, concurrent-apply tests, fuzz\n+ bench workflows, walker WalkParallel switch, label_for_path fast\npath, from_apply header collapse, lockfile error classification,\ncanonical workspace lock derivation, and the EXDEV rename fallback.",
          "timestamp": "2026-05-22T17:42:53+03:00",
          "tree_id": "1f7465fdac4541ba94f14fe138a0e07740848d96",
          "url": "https://github.com/Stoica-Mihai/recast/commit/5598fafc2ae4df548843204bb0758161b22e6109"
        },
        "date": 1779461136027,
        "tool": "cargo",
        "benches": [
          {
            "name": "pattern_compile_simple",
            "value": 2360,
            "range": "± 222",
            "unit": "ns/iter"
          },
          {
            "name": "pattern_compile_complex",
            "value": 619626,
            "range": "± 10994",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/10_files",
            "value": 1679617,
            "range": "± 143604",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/100_files",
            "value": 2452557,
            "range": "± 86372",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/500_files",
            "value": 3828455,
            "range": "± 153260",
            "unit": "ns/iter"
          },
          {
            "name": "structural_rewrite_rename_one_identifier",
            "value": 3175926,
            "range": "± 20123",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/10_files",
            "value": 3414444,
            "range": "± 181143",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/100_files",
            "value": 4557566,
            "range": "± 138709",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/500_files",
            "value": 7524582,
            "range": "± 247072",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "ad7d2adcfb26280bd88112ece5f7ee6d31577f65",
          "message": "feat(mcp): close CLI parity gaps — Rhai scripts + friendly ast_pattern\n\nRewriteArgs gains `script_source: Option<String>` + `script_path:\nOption<PathBuf>`, mutually exclusive. When either is set, the tool\nroutes through plan_rewrite_scripted and the regex match's\nreplacement comes from the Rhai callback instead of the static\ntemplate. Closes the CLI's `--script PATH` gap.\n\nStructuralArgs gains `ast_pattern: Option<String>`. Exactly one of\n`query` / `ast_pattern` is now required; when `ast_pattern` is set,\nthe engine compiles it via compile_friendly_query before planning.\nCloses the CLI's `--ast` gap so agents can write\n`fn $NAME() {}` instead of memorizing tree-sitter S-expressions.\n\nTool bodies route through new `plan_for(&args)` helper and a small\n`invalid_args` constructor that distinguishes caller-side mistakes\n(mutual-exclusion violations, missing required fields) from\nengine-side errors in the MCP error payload.\n\nSmoke-verified end-to-end:\n- recast_structural with ast_pattern + ${NAME} template renames `fn foo() {}`\n  to `fn foo_v2() {}` in 1 match across 1 file.\n- recast_preview with script_source `(parse_int(captures[0]) + 1).to_string()`\n  on a `\\d+` pattern correctly increments and surfaces the\n  non-convergent guard (kind: \"non_convergent\") in the McpError data.\n\nRemaining CLI flags without MCP equivalents — stdin, completions,\ndiff/quiet/verbose, force, threads — are either pipe-shaped\n(meaningless in MCP), output-knob style (MCP is always structured\nJSON), or server-controlled. No real parity gap left.",
          "timestamp": "2026-05-22T17:50:11+03:00",
          "tree_id": "693d335ae76d9829797eddd71b04ebdde581e6a0",
          "url": "https://github.com/Stoica-Mihai/recast/commit/ad7d2adcfb26280bd88112ece5f7ee6d31577f65"
        },
        "date": 1779461563753,
        "tool": "cargo",
        "benches": [
          {
            "name": "pattern_compile_simple",
            "value": 2398,
            "range": "± 59",
            "unit": "ns/iter"
          },
          {
            "name": "pattern_compile_complex",
            "value": 622895,
            "range": "± 6688",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/10_files",
            "value": 1787032,
            "range": "± 168098",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/100_files",
            "value": 2389638,
            "range": "± 107406",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/500_files",
            "value": 3626663,
            "range": "± 139730",
            "unit": "ns/iter"
          },
          {
            "name": "structural_rewrite_rename_one_identifier",
            "value": 3193531,
            "range": "± 40072",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/10_files",
            "value": 3443309,
            "range": "± 128540",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/100_files",
            "value": 4514320,
            "range": "± 146140",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/500_files",
            "value": 7228475,
            "range": "± 181184",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "0db4938962b83221697f9fd0be495d650980fc6c",
          "message": "test(mcp): in-process unit tests for tool handlers\n\n11 #[tokio::test]s in crates/recast-mcp/src/server_tests.rs call\neach tool handler directly with constructed Parameters<T> values —\nno JSON-RPC framing, no transport, no subprocess. Faster than the\nstdio smoke tests and runs in the normal `cargo test` suite.\n\nCoverage:\n- preview emits plan JSON, dry-run leaves files untouched\n- apply writes new content to disk, returns apply JSON\n- zero matches on a convergent rewrite returns already_applied\n  (success outcome, not a guard violation)\n- at_least > actual matches surfaces TooFewMatches with\n  kind=\"too_few_matches\" in McpError data\n- non-convergent rewrite (a -> aa) surfaces NonConvergent with\n  kind=\"non_convergent\"\n- script_source drives Rhai callback per match\n- mutual exclusion: script_source + script_path is rejected\n- structural ast_pattern compiles to query and runs end-to-end\n- mutual exclusion: query + ast_pattern is rejected\n- requires one of query / ast_pattern\n- recover with no leftovers returns zero summary\n\nTotal test count 136 -> 147.",
          "timestamp": "2026-05-22T17:57:50+03:00",
          "tree_id": "378f32a851ffa20a5c592b1317a57b39993d2a08",
          "url": "https://github.com/Stoica-Mihai/recast/commit/0db4938962b83221697f9fd0be495d650980fc6c"
        },
        "date": 1779462025024,
        "tool": "cargo",
        "benches": [
          {
            "name": "pattern_compile_simple",
            "value": 2323,
            "range": "± 79",
            "unit": "ns/iter"
          },
          {
            "name": "pattern_compile_complex",
            "value": 608020,
            "range": "± 4063",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/10_files",
            "value": 1490886,
            "range": "± 96091",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/100_files",
            "value": 2127067,
            "range": "± 143831",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/500_files",
            "value": 3392171,
            "range": "± 141662",
            "unit": "ns/iter"
          },
          {
            "name": "structural_rewrite_rename_one_identifier",
            "value": 3168376,
            "range": "± 24384",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/10_files",
            "value": 3284007,
            "range": "± 114784",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/100_files",
            "value": 4250817,
            "range": "± 164243",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/500_files",
            "value": 6975865,
            "range": "± 211853",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "d17e28ba2948285c849811c7b8ef92bdd87dc3bf",
          "message": "chore: bump to 0.1.9",
          "timestamp": "2026-05-22T18:02:46+03:00",
          "tree_id": "e8bbe7dcf825234a7e1f897ba29f22e892d19650",
          "url": "https://github.com/Stoica-Mihai/recast/commit/d17e28ba2948285c849811c7b8ef92bdd87dc3bf"
        },
        "date": 1779462343553,
        "tool": "cargo",
        "benches": [
          {
            "name": "pattern_compile_simple",
            "value": 2390,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "pattern_compile_complex",
            "value": 622884,
            "range": "± 10981",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/10_files",
            "value": 1684098,
            "range": "± 157063",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/100_files",
            "value": 2530060,
            "range": "± 92292",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/500_files",
            "value": 3812440,
            "range": "± 122023",
            "unit": "ns/iter"
          },
          {
            "name": "structural_rewrite_rename_one_identifier",
            "value": 3198033,
            "range": "± 49961",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/10_files",
            "value": 3439515,
            "range": "± 142693",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/100_files",
            "value": 4585325,
            "range": "± 157804",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/500_files",
            "value": 7362591,
            "range": "± 180767",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "3d5b5a1ed0d5cd181efc04a47e073a521a949f29",
          "message": "chore: bump to 0.1.10 — dedicated recast-mcp README on crates.io\n\nrecast-mcp's crates.io page was rendering the CLI-focused root\nREADME (it told visitors to `cargo install recast-cli`). Added\ncrates/recast-mcp/README.md with MCP-specific install, Claude\nDesktop / Cursor config snippets, the four tools, and the safety\npitch. Pointed crates/recast-mcp/Cargo.toml at the new file.\n\nWorkspace version bumped to 0.1.10 so the new README ships to\ncrates.io on the next release. recast-cli and recast-core\nrepublish alongside (no material change in either); they continue\nto share the root README, which is close enough to their actual\naudience.",
          "timestamp": "2026-05-22T18:12:48+03:00",
          "tree_id": "1ca02c15d5244bd55f7429bc1757dddaf710bbb6",
          "url": "https://github.com/Stoica-Mihai/recast/commit/3d5b5a1ed0d5cd181efc04a47e073a521a949f29"
        },
        "date": 1779462939447,
        "tool": "cargo",
        "benches": [
          {
            "name": "pattern_compile_simple",
            "value": 2405,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "pattern_compile_complex",
            "value": 618218,
            "range": "± 3864",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/10_files",
            "value": 1682817,
            "range": "± 124002",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/100_files",
            "value": 2423639,
            "range": "± 93571",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/500_files",
            "value": 3805426,
            "range": "± 121019",
            "unit": "ns/iter"
          },
          {
            "name": "structural_rewrite_rename_one_identifier",
            "value": 3203090,
            "range": "± 19536",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/10_files",
            "value": 3446264,
            "range": "± 132079",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/100_files",
            "value": 4598873,
            "range": "± 147820",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/500_files",
            "value": 7472476,
            "range": "± 243502",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "6ab56392f4e4352d1d5c7ddf7dac080cf03c4963",
          "message": "chore: bump to 0.1.11",
          "timestamp": "2026-05-23T15:21:04+03:00",
          "tree_id": "603c7b33dad340febc041b60e9d1e6c7aa44f5cb",
          "url": "https://github.com/Stoica-Mihai/recast/commit/6ab56392f4e4352d1d5c7ddf7dac080cf03c4963"
        },
        "date": 1779539490661,
        "tool": "cargo",
        "benches": [
          {
            "name": "pattern_compile_simple",
            "value": 2322,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "pattern_compile_complex",
            "value": 623166,
            "range": "± 6733",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/10_files",
            "value": 1634912,
            "range": "± 139577",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/100_files",
            "value": 2385584,
            "range": "± 119547",
            "unit": "ns/iter"
          },
          {
            "name": "plan_rewrite/500_files",
            "value": 3705017,
            "range": "± 143941",
            "unit": "ns/iter"
          },
          {
            "name": "structural_rewrite_rename_one_identifier",
            "value": 3164594,
            "range": "± 47786",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/10_files",
            "value": 3360888,
            "range": "± 120114",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/100_files",
            "value": 4528314,
            "range": "± 158821",
            "unit": "ns/iter"
          },
          {
            "name": "plan_structural_rewrite/500_files",
            "value": 7279774,
            "range": "± 227983",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}
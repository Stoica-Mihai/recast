# Operating manual

The full operating manual for AI agents and human contributors lives in
[`AGENTS.md`](https://github.com/Stoica-Mihai/recast/blob/main/AGENTS.md)
(symlinked as `CLAUDE.md` for agent runtimes that look it up by that
name).

Highlights:

- TDD by default — every feature goes through `/tdd:tdd`
- DRY enforced — every feature/fix runs through `/engineering-principles:dry-principle`
- Tests live in their own files (`foo_tests.rs` next to `foo.rs`)
- Property tests via `proptest` for every public entry point
- Snapshot tests via `insta` for JSON + unified-diff output
- Conventional commit prefixes; one concern per PR
- No `Co-Authored-By` trailers
- README + CHANGELOG + PLAN are documentation contracts — drift is a bug
- Release process is fully automated from tag push

See the canonical file for the full set of rules and the reasoning
behind each one.

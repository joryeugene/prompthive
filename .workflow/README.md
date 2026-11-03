# Release acceptance

Normal release preparation uses `bash scripts/release --check`. Publication uses the same gate through `bash scripts/release --publish`, only after approval of the tested scope.

Before any Cargo release action, `.workflow/check-release --stage local` calls the canonical acceptance checker in the release workspace. The default workspace evidence lives in the parent `.workflow` directory; an isolated checkout can set `PROMPTHIVE_ACCEPTANCE_ROOT` explicitly. Missing, incomplete, or stale acceptance stops release. The canonical source manifest must include the complete candidate source, fixtures, and verifiers. The evidence ledger remains private to its workspace; this repository contains no user prompt data or copied credentials.

CI runs a negative sentinel test: missing acceptance must stop both normal modes before even a fake Cargo command can run. This tests release wiring; it does not establish actual release acceptance. Hosted builds, native clipboard/platform observations, exact-head review, package inspection, and approval remain separate requirements. Direct manual Cargo/GitHub operations are outside this script's interception.

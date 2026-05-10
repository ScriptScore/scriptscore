# Agent Guidance

This repository is a public preview of ScriptScore, an open-source desktop and
CLI client for exam grading workflows. Keep changes small, tested, and aligned
with the public client boundary.

## Architecture Boundaries

- `cli/` contains the Python package, direct CLI, JSON-RPC sidecar, public
  command contracts, providers, prompts, artifacts, and tests.
- `desktop/frontend/` contains the Svelte/SvelteKit UI. It should call typed
  Tauri/frontend helpers and must not call Python directly.
- `desktop/src-tauri/` contains the Rust/Tauri host. Rust owns desktop state,
  project persistence, worker lifecycle, background jobs, cancellation, and
  privacy boundaries for durable local state.
- Python is the bounded execution worker for grading/runtime commands. Keep
  direct CLI and sidecar behavior in parity when changing command behavior.
- `desktop/scripts/` and `scripts/` contain build, development, legal, and
  compliance helpers. Prefer existing helpers over ad hoc scripts.

## Public Boundary

- Do not add hosted service implementation details, private package loading, or
  private service internals. ScriptScorePlus work in this repository must remain
  explicit client-side hosted API support.
- Do not commit real student data, raw submissions, local project databases,
  credentials, API keys, generated traces, or screenshots containing real data.
  Tests and examples must use synthetic, safe-to-publish fixtures.
- Review license and dependency impact before adding dependencies, bundled
  assets, native binaries, models, or generated legal artifacts.
- Keep `desktop/dist/`, coverage output, local models, package artifacts,
  caches, and machine-local outputs out of commits.

## Development Commands

Run narrow checks that cover the change.

Python CLI:

```bash
cd cli
uv sync --group dev
uv run ruff check .
uv run ruff format --check .
uv run mypy
uv run pytest -q --cov
uv build
```

Frontend:

```bash
cd desktop/frontend
npm ci
npm run lint
npm run check
npm test
npm run build
```

Rust/Tauri host:

```bash
cd desktop/src-tauri
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --all-targets
```

Broad cross-layer review:

```bash
make review-quality
python scripts/check_spdx_headers.py
python desktop/scripts/check_scriptscoreplus_boundary.py
```

On Windows, prefer:

```powershell
powershell -ExecutionPolicy Bypass -File .\desktop\scripts\review-quality.ps1
```

## Testing Expectations

- Add or update focused tests for behavior changes.
- For CLI command changes, check direct CLI and sidecar/desktop-worker parity
  when the command is exposed through both paths.
- For artifact-producing commands, keep writes under caller-provided
  `output_artifacts_dir` and avoid leaking machine-local paths in public data.
- For frontend changes, prefer existing Vitest and Svelte Testing Library
  patterns in `desktop/frontend/src/**/*.test.ts`.
- For Rust state, persistence, workflow, or worker changes, use existing unit
  tests and `desktop/src-tauri/tests/state_integration.rs` patterns.

## Style Notes

- Python targets Python 3.12+, uses Ruff formatting with 100-character lines,
  and runs strict mypy for `src/scriptscore` and tests.
- Frontend code uses Svelte 5, TypeScript, Vite, Vitest, and ESLint.
- Rust uses the pinned toolchain in `rust-toolchain.toml`; run strict clippy
  before pushing Rust changes.
- Every commit must include a DCO sign-off. Use `git commit -s`.

## Platform And Release Notes

Linux is the verified desktop packaging target for the current public preview.
Windows and macOS runtime/installer paths are in progress and should be treated
as unverified unless a dedicated platform validation path says otherwise.

Release packaging and private wheel consumption must not introduce runtime
downloads that require private URLs or tokens. Public source should contain
configuration hooks and checks only, not private release assets or secrets.

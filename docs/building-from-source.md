# Building From Source

These commands are intended for a clean public checkout.

## Platform Status

Linux is the verified desktop packaging target for the current public preview. Windows and macOS build/runtime paths are in progress and unverified; installer generation for those platforms should stay disabled until dedicated platform testing is complete.

## Python CLI

```bash
cd cli
uv sync --group dev
uv run ruff check .
uv run ruff format --check .
uv run mypy
uv run pytest -q --cov
uv build
```

## Frontend

```bash
cd desktop/frontend
npm ci
npm run lint
npm run check
npm test
npm run build
```

## Rust/Tauri Host

```bash
cd desktop/src-tauri
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --all-targets
```

These host checks are general development checks. Passing them does not mean Windows or macOS desktop installers have been validated.

## Legal Artifacts

Before distributing release artifacts, generate dependency notices and inventory:

```bash
python desktop/scripts/generate_legal_artifacts.py
```

The generated files are placed under `desktop/dist/legal/` for desktop bundling and release review.
Do not commit `desktop/dist/`; it is generated output and may contain machine-local paths.

## Optional Quality Reports

The scripts under `desktop/scripts/` include optional local quality-report helpers, including
SonarCloud issue export when `SONAR_TOKEN`, `SONAR_ORGANIZATION`, and `SONAR_PROJECT_KEY` are
provided. These reports are not required for a public source checkout and write generated output
under ignored artifact directories.

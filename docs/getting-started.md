# Getting Started

ScriptScore is an open-source client for exam grading workflows. The public preview includes both a Python CLI and a Tauri desktop app.

## Prerequisites

- Python 3.12 or 3.13.
- `uv` for Python dependency management.
- Node.js 20 for the frontend.
- Rust stable for the Tauri host.
- System Tauri dependencies for your operating system.

## Platform Status

Linux is the currently verified desktop packaging target for the public preview. Windows and macOS desktop runtime and installer paths are in progress and unverified; do not treat them as release-ready until platform-specific testing is complete.

## CLI Smoke Test

```bash
cd cli
uv sync --group dev
uv run scriptscore smoke.ping --request-json '{"message":"hello"}'
```

## Frontend Check

```bash
cd desktop/frontend
npm ci
npm run check
npm test
```

## Desktop Host Check

```bash
cd desktop/src-tauri
cargo test --all-targets
```

## Next Reading

- `docs/provider-modes.md`
- `docs/privacy-and-data.md`
- `docs/building-from-source.md`

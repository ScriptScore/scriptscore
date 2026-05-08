# Contributing

ScriptScore is in public preview. Keep changes small, tested, and aligned with the current client boundary.

## Setup

```bash
cd cli && uv sync --group dev
cd ../desktop/frontend && npm ci
cd ../src-tauri && cargo test --all-targets
```

## Before Opening A Pull Request

Run the narrowest checks that cover your change. For broad or cross-layer changes, run:

```bash
make review-quality
python scripts/check_spdx_headers.py
python desktop/scripts/check_scriptscoreplus_boundary.py
```

For Rust/Tauri changes, run the same clippy command enforced by CI before pushing:

```bash
cd desktop/src-tauri
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

On Windows, use the PowerShell review helper instead of the Unix-oriented `make` target:

```powershell
powershell -ExecutionPolicy Bypass -File .\desktop\scripts\review-quality.ps1
```

The Windows helper includes the strict Rust clippy check used above. Native Windows clippy does not evaluate Linux-only `cfg` paths, so keep CI/Linux as the final signal for platform-specific warnings; if your local toolchain supports extra targets, pass them with `-RustClippyTargets`.

The Windows helper skips `cargo-geiger` by default because full `cargo-geiger` performs a clean Rust rebuild before scanning. On the desktop/Tauri dependency graph this is currently impractical on Windows and may leave an empty JSON report until the command eventually completes. Use CI/Linux for the full `unsafe-report` target; pass `-IncludeUnsafeReport` only when you intentionally want to attempt the long Windows run.

## DCO

Every commit must include a Developer Certificate of Origin sign-off:

```text
Signed-off-by: Your Name <you@example.com>
```

Use `git commit -s` when committing.

## Privacy

Do not commit real student data, raw submissions, local project databases, credentials, generated traces, or screenshots containing real data. Test fixtures must be synthetic and safe to publish.

## ScriptScorePlus Boundary

Do not add hosted service implementation details, private package loading, or private service internals to this repository. Client-side hosted API support must remain explicit and safe for the public client.

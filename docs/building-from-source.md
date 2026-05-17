# Building From Source

These commands are intended for a clean public checkout.

## Platform Status

Linux is the verified desktop packaging target for the current public preview. Windows and macOS build/runtime paths are in progress and unverified; installer generation for those platforms should stay disabled until dedicated platform testing is complete.

## Windows Development

Windows desktop development uses PowerShell helpers because the top-level `Makefile` and several quality scripts assume a Unix shell. Run the Windows tool bootstrap first if the repo-local tools are missing:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\windows-dev.ps1
```

The Windows helpers prefer these repo-local tools when present:

- `C:\scriptscore\.tools\uv\uv.exe`
- `C:\scriptscore\.tools\node-v20.20.2-win-x64\npm.cmd`
- `C:\scriptscore\.tools\mingit\cmd\git.exe`
- `cli\.venv\Scripts\python.exe`

To run the review-quality equivalent on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\desktop\scripts\review-quality.ps1
```

This runs the formatting, lint, CLI, frontend, Rust coverage, Rust metrics, and license checks that are practical on Windows. `cargo-geiger` is opt-in because full `cargo-geiger` is currently impractical in the Windows development environment:

- full mode performs a `cargo clean` before checking the workspace
- the desktop/Tauri dependency graph is large enough that the clean rebuild can run for hours on Windows
- JSON output is written only after the scan completes, so the report file may remain empty while the command appears hung

For day-to-day Windows review, leave the unsafe report disabled and rely on CI/Linux for the full `unsafe-report` target. If you need a quick local signal that avoids the clean rebuild, run the helper with `-IncludeUnsafeReport` only when you are prepared for a long run, or run `cargo geiger --forbid-only --output-format Json` manually for source-entry-point coverage.

```powershell
powershell -ExecutionPolicy Bypass -File .\desktop\scripts\review-quality.ps1 -IncludeUnsafeReport
```

Use `-CheckPrerequisitesOnly` for a quick toolchain sanity check, and `-SkipRustCoverage` when iterating on unrelated changes.

The Windows helper runs strict Rust clippy locally with `--workspace --all-targets --all-features -- -D warnings`. Native Windows clippy does not evaluate Linux-only `cfg` paths; use CI/Linux for that signal, or pass additional configured targets with `-RustClippyTargets` when your local toolchain supports them.

PowerShell text redirection can write encodings that break JSON report parsing. The Windows review helper uses byte-preserving `cmd.exe` redirection for `rust-code-analysis-cli`; prefer the helper over manually redirecting those reports in PowerShell.

To install PaddleOCR models on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\cli\scripts\install_paddle_models.ps1
```

The default destination is `cli\models\paddle`, which is one of the desktop development fallback locations. The installed layout matches the Linux installer:

```text
cli\models\paddle\
  det\
    inference.yml
    inference.pdmodel or inference.json
    inference.pdiparams
  rec\
    inference.yml
    inference.pdmodel or inference.json
    inference.pdiparams
```

Pass a destination path or set `PADDLE_MODEL_ROOT` to install elsewhere. For CLI tests that need the models, set:

```powershell
$env:SCRIPTSCORE_TEST_PII_PADDLE_MODEL_DIR = "C:\path\to\models\paddle"
```

When passing Paddle model paths to the desktop worker, use normal Windows paths such as `C:\...\models\paddle`. Avoid verbatim paths such as `\\?\C:\...\models\paddle`; Python can see those paths, but PaddleOCR native loaders may reject them.

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

## Desktop Preview Packages

The `Desktop Preview Packages` GitHub Actions workflow builds preview desktop
packages across the platform matrix and publishes them to GitHub Releases with
CI-managed release tags. Maintainers do not need to create semver tags or bump
checked-in prerelease versions before running the workflow.

Successful `CI` runs for push events on `main` and `release/**` publish the
`latest` preview channel. Failed CI runs, pull request CI runs, and manually
dispatched CI runs do not start preview package publishing. Manual workflow
dispatch can publish either `latest` or `rc`, and can limit the build to one
package platform while iterating. The workflow computes versions from the
checked-in base desktop version plus the GitHub Actions run number, for example
`0.1.0-latest.123` or `0.1.0-rc.124`. GitHub Releases require tags, so the
workflow owns internal tags such as the moving `ci/latest` tag and immutable RC
tags like `ci/rc/0.1.0-rc.124`; those tags are implementation details and
should not be created manually. The moving `ci/latest` release replaces its
asset set on each successful branch build, while RC releases keep their
versioned per-run assets. Manual `latest` runs only publish to `ci/latest` when
`package_platform=all`; single-platform manual runs retain their workflow
artifacts without replacing the full latest release asset set. Automatic
`latest` publishing also verifies that the successful CI commit is still the
source branch tip before moving `ci/latest`.

The workflow builds:

- unsigned preview DMGs on macOS Intel and macOS Apple Silicon,
- NSIS and MSI packages on Windows x64,
- AppImage, deb, and rpm packages on Linux x64.

macOS Intel packaging requires the private PaddlePaddle wheel release asset at
build time. Because that release asset is private, configure repository or
organization secrets/variables in the public ScriptScore repository rather than
hard-coding private release coordinates in source:

```text
SCRIPTSCORE_PYTHON_WHEELS_READ_TOKEN
SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_REPOSITORY
SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_RELEASE_TAG
SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_SHA256
```

Use repository variables for the private release repository and tag if exposing
those names to users with workflow access is acceptable; use secrets instead if
they should remain hidden in logs and workflow UI. A direct asset URL is also
supported through `SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_URL`, with
`SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_TOKEN` as the optional bearer token. If
that separate token is not set, the workflow reuses
`SCRIPTSCORE_PYTHON_WHEELS_READ_TOKEN`.

Only the CI job uses those values. The packaged desktop runtime must contain
the installed wheel contents and must not contain the private URL, token, or
any runtime wheel-download requirement. The macOS Intel runtime is PaddleOCR
only for this RC package path; EasyOCR, Torch, and TorchVision are excluded
there because the locked PyTorch stack does not publish macOS Intel CPython
3.12 wheels. Formal Apple Developer ID signing, notarization, Windows code
signing, and Linux repository signing are deliberately outside this first RC
package workflow.

## Optional Quality Reports

The scripts under `desktop/scripts/` include optional local quality-report helpers, including
SonarCloud issue export when `SONAR_TOKEN`, `SONAR_ORGANIZATION`, and `SONAR_PROJECT_KEY` are
provided. These reports are not required for a public source checkout and write generated output
under ignored artifact directories.

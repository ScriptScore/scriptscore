# ScriptScore Desktop

The desktop app combines a Svelte/SvelteKit frontend with a Rust/Tauri host and the public Python CLI/runtime sidecar.

## Frontend

```bash
cd frontend
npm ci
npm run check
npm test
npm run build
```

## Tauri Host

```bash
cd src-tauri
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --all-targets
```

## Development

Use the scripts under `desktop/scripts/` for local desktop development and packaging experiments. Public preview packaging must include generated legal artifacts under `desktop/dist/legal/`.

On Linux, launch the desktop app with:

```bash
./desktop/scripts/dev-desktop.sh
```

On Windows, launch the desktop app with:

```powershell
powershell -ExecutionPolicy Bypass -File .\desktop\scripts\dev-desktop.ps1
```

The Windows launcher does not require `cargo-tauri`; it starts the Vite frontend, builds the Rust host with Cargo, opens the desktop app, and cleans up the frontend server it started when the app exits.

For Windows review checks, run:

```powershell
powershell -ExecutionPolicy Bypass -File .\desktop\scripts\review-quality.ps1
```

This is the PowerShell equivalent for the review-quality checks that can run reliably on Windows, including strict Rust clippy. Native Windows clippy does not evaluate Linux-only `cfg` paths; use CI/Linux for that signal, or pass extra configured targets with `-RustClippyTargets`. The unsafe report is opt-in with `-IncludeUnsafeReport` because full `cargo-geiger` performs a clean Rust rebuild before scanning; on the desktop/Tauri dependency graph that is currently impractical in the Windows development toolchain and can look hung while the JSON report remains empty.

Linux is the only verified desktop packaging target for the current public preview. Windows desktop runtime development is in progress, and Windows installer packaging remains unverified. macOS desktop runtime and installer paths are in progress and unverified.

### macOS Desktop Development

Use the macOS-specific launcher when testing the Tauri desktop app on macOS:

```bash
./desktop/scripts/dev-desktop-macos.sh
```

The launcher starts the Svelte/Vite frontend explicitly, waits for it to become
available, then starts the Tauri host against that dev server. This avoids
platform-specific differences in Tauri's nested `beforeDevCommand` handling
during local macOS development.

If `npm` is not on `PATH`, the launcher first checks `SCRIPTSCORE_NODE_BIN` and
then a workspace-local `.tools/node-*-darwin-*/bin` directory. For example:

```bash
SCRIPTSCORE_NODE_BIN="/path/to/node/bin" ./desktop/scripts/dev-desktop-macos.sh
```

This launcher is for local development only. It does not make macOS desktop
packaging release-ready.

## Provider Modes

The desktop UI supports no-AI and Ollama-oriented workflows. ScriptScorePlus appears only as planned hosted API support and is not required for local use.

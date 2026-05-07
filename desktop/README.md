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

Linux is the only verified desktop packaging target for the current public preview. Windows desktop runtime development is in progress, and Windows installer packaging remains unverified. macOS desktop runtime and installer paths are in progress and unverified.

## Provider Modes

The desktop UI supports no-AI and Ollama-oriented workflows. ScriptScorePlus appears only as planned hosted API support and is not required for local use.

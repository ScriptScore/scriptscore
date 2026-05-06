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

Linux is the only verified desktop packaging target for the current public preview. Windows and macOS desktop runtime and installer paths are in progress and unverified.

## Provider Modes

The desktop UI supports no-AI and Ollama-oriented workflows. ScriptScorePlus appears only as planned hosted API support and is not required for local use.

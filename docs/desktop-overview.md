# Desktop Overview

The ScriptScore desktop app is a Tauri application with a Svelte/SvelteKit frontend and Rust host. It uses the public Python CLI/runtime for grading workflow operations through explicit host-side boundaries.

## Components

- `desktop/frontend/`: Svelte UI, workflow screens, settings, and browser-preview development surface.
- `desktop/src-tauri/`: Rust host, command bindings, local state, resource access, and sidecar coordination.
- `cli/`: Python runtime and command implementation used by desktop workflows.

## Public Preview Notes

The desktop app is prerelease. Storage, packaging, and workflow details may change before a stable release. ScriptScorePlus hosted API support is planned and is not required for no-AI or Ollama workflows.

Linux packaging is the only verified desktop packaging path in the public preview. Windows and macOS desktop runtime and installer support is in progress and should be treated as unverified.

## Legal Notices

Packaged desktop builds should include generated notices under `desktop/dist/legal/`. The Settings workspace includes an open-source license disclosure surface for bundled notices.

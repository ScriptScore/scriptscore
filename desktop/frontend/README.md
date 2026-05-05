# Frontend

The desktop frontend is a Svelte/SvelteKit app rendered inside the Tauri desktop host.

Responsibilities:

- Application shell, navigation, and workflow UI.
- Local optimistic UI state and transient selections.
- Tauri command calls through typed frontend helpers.
- No direct Python calls from frontend code.

The frontend talks to the Rust host. The Rust host owns project state, persistence, background jobs, and Python sidecar lifecycle.

## Development

```bash
npm ci
npm run check
npm test
npm run build
```

Use the scripts in `desktop/scripts/` from the repo root for integrated desktop development.

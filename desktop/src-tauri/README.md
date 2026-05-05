# src-tauri

This directory is reserved for the Tauri/Rust host.

Ownership:

- launch and supervise the Python worker
- local socket / named-pipe transport
- job lifecycle, cancellation, and progress normalization
- SQLite project state and migrations
- privacy boundary enforcement for durable desktop state

Rust is the desktop control plane. Python is the bounded execution worker.

Phase 01 currently provides:

- `Cargo.toml`, `tauri.conf.json`, and the default capability file
- worker supervision and framed desktop sidecar client
- Rust-owned project creation/open and baseline SQLite schema bootstrap
- Tauri commands for shell state, project open/create, and `smoke.ping`
- the canonical host dev path through `desktop/scripts/dev-desktop.sh`
- a no-browser Vite helper for Tauri under `desktop/scripts/dev-vite.sh`

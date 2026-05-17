# ScriptScore

[![CI](https://github.com/ScriptScore/scriptscore/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/ScriptScore/scriptscore/actions/workflows/ci.yml)
[![License Compliance](https://github.com/ScriptScore/scriptscore/actions/workflows/license-compliance.yml/badge.svg?branch=main)](https://github.com/ScriptScore/scriptscore/actions/workflows/license-compliance.yml)
[![Desktop Preview Packages](https://github.com/ScriptScore/scriptscore/actions/workflows/release-packages.yml/badge.svg?branch=main)](https://github.com/ScriptScore/scriptscore/actions/workflows/release-packages.yml)
[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=ScriptScore_scriptscore&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=ScriptScore_scriptscore)
[![License: AGPL-3.0-only](https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg)](LICENSE)

ScriptScore is an open-source desktop and CLI client for exam grading workflows. The public preview includes the Python CLI/runtime, JSON-RPC sidecar, Tauri desktop host, and Svelte/SvelteKit frontend.

ScriptScore is designed so grading workflows can run without hosted AI. The client supports no-AI workflows and local Ollama workflows, and includes the client-side foundation for hosted Ollama and future ScriptScorePlus hosted API use.

## Status

This repository is a public preview. Interfaces, desktop storage, packaging, and provider flows may change before a stable release. ScriptScorePlus hosted API support is planned and is not required for local use.

Desktop packaging is currently verified only for Linux preview builds. Windows and macOS desktop packaging/runtime support is in progress and should be treated as unverified until dedicated platform testing is complete.

## Provider Modes

- No AI assistance: use ScriptScore workflow tools without model-backed assistance.
- Local Ollama: run inference against an Ollama server on your own machine or network.
- Hosted Ollama: connect to a hosted Ollama endpoint when configured by the user.
- ScriptScorePlus: planned hosted API service. The server implementation is not included in this repository.

## Repository Layout

```text
cli/                  Python package, CLI, JSON-RPC sidecar, shared runtime
desktop/frontend/    Svelte/SvelteKit desktop frontend
desktop/src-tauri/   Rust/Tauri desktop host
desktop/scripts/     Desktop build, legal, and quality helpers
docs/                Curated public documentation
scripts/             Repo-level contribution and compliance helpers
```

## Quick Start

Install CLI dependencies:

```bash
cd cli
uv sync --group dev
uv run scriptscore smoke.ping --request-json '{"message":"hello"}'
```

Run frontend checks:

```bash
cd desktop/frontend
npm ci
npm run check
npm test
```

Run Rust host checks:

```bash
cd desktop/src-tauri
cargo fmt --check
cargo test --all-targets
```

See [Getting Started](docs/getting-started.md) and [Building From Source](docs/building-from-source.md) for fuller setup instructions.

## Documentation

- [Getting Started](docs/getting-started.md)
- [Building From Source](docs/building-from-source.md)
- [CLI Reference](docs/cli-reference.md)
- [Desktop Overview](docs/desktop-overview.md)
- [Provider Modes](docs/provider-modes.md)
- [Privacy And Data](docs/privacy-and-data.md)
- [Dependency Policy](docs/licensing/dependency-policy.md)
- [Release Compliance](docs/licensing/release-compliance.md)

## Privacy

ScriptScore is intended for sensitive exam workflows. Do not commit student data, raw submissions, local databases, credentials, API keys, generated traces, or screenshots containing real data. See [Privacy And Data](docs/privacy-and-data.md).

## License

ScriptScore client source is licensed under `AGPL-3.0-only`. See `LICENSE` and `NOTICE`.

The client currently depends on PyMuPDF/MuPDF through the CLI runtime. While that dependency remains in the distributed client stack, release planning treats the public client as AGPL-3.0-only and requires corresponding source, notices, dependency inventory, and binary distribution review.

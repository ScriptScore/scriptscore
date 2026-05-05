# ScriptScore CLI

The `cli/` project contains the Python `scriptscore` package, command-line entrypoint, JSON-RPC sidecar, provider interfaces, public command contracts, prompts, and tests.

## Setup

```bash
uv sync --group dev
```

## Smoke Test

```bash
uv run scriptscore smoke.ping --request-json '{"message":"hello"}'
```

CLI commands read structured JSON requests and write machine-readable JSON envelopes. Artifact-producing commands write under the caller-supplied `output_artifacts_dir`.

## Quality Checks

```bash
uv run ruff check .
uv run ruff format --check .
uv run mypy
uv run pytest -q --cov
```

## Providers

The public CLI includes no-AI/test providers and Ollama provider support. ScriptScorePlus hosted API support is planned client-side work; the hosted service implementation is not part of this package.

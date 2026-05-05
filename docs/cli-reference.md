# CLI Reference

The ScriptScore CLI accepts structured JSON requests and returns machine-readable JSON envelopes. Direct CLI invocation and the JSON-RPC sidecar share the same command implementation path.

## Command Groups

Current public command groups include:

- `smoke.*` for smoke checks.
- `runtime.*` for model listing and validation.
- `exam.*` for exam setup, analysis, and rubric generation.
- `scans.*` for scan ingestion, canonicalization, alignment, detection, cropping, PII prescreening, OCR, parsing, and PDF helpers.
- `grading.*` for preliminary scoring, consistency review, feedback drafting, markup, and export.

## Request And Output Model

Commands receive a JSON request payload and return either a success envelope or an error envelope. Commands that write artifacts must write under the caller-provided `output_artifacts_dir`.

## Smoke Example

```bash
cd cli
uv run scriptscore smoke.ping --request-json '{"message":"hello"}'
```

This document is intentionally high level for the public preview. Use the CLI help output and tests as implementation-backed references while the command surface stabilizes.

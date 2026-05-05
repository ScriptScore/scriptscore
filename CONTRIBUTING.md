# Contributing

ScriptScore is in public preview. Keep changes small, tested, and aligned with the current client boundary.

## Setup

```bash
cd cli && uv sync --group dev
cd ../desktop/frontend && npm ci
cd ../src-tauri && cargo test --all-targets
```

## Before Opening A Pull Request

Run the narrowest checks that cover your change. For broad or cross-layer changes, run:

```bash
make review-quality
python scripts/check_spdx_headers.py
python desktop/scripts/check_scriptscoreplus_boundary.py
```

## DCO

Every commit must include a Developer Certificate of Origin sign-off:

```text
Signed-off-by: Your Name <you@example.com>
```

Use `git commit -s` when committing.

## Privacy

Do not commit real student data, raw submissions, local project databases, credentials, generated traces, or screenshots containing real data. Test fixtures must be synthetic and safe to publish.

## ScriptScorePlus Boundary

Do not add hosted service implementation details, private package loading, or private service internals to this repository. Client-side hosted API support must remain explicit and safe for the public client.

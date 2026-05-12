# Dependency Policy

ScriptScore first-party client source is licensed under `AGPL-3.0-only`.

## Current Licensing Posture

The CLI runtime currently depends on PyMuPDF, which is tied to MuPDF licensing. While PyMuPDF/MuPDF remains in the distributed client runtime stack, public release planning treats the ScriptScore client as AGPL-3.0-only.

If PyMuPDF is removed, isolated from distributed runtime artifacts, or otherwise replaced later, the project may revisit its license strategy.

## Dependency Classes

Permissive dependencies such as MIT, BSD, ISC, and Apache-2.0 are generally compatible with the current AGPL client posture. MPL-2.0 dependencies may be acceptable with file-level notice obligations. GPL/LGPL, unknown, native binary, model, and custom-license dependencies require explicit release review.

Review decisions live in `docs/licensing/dependency-review-register.md`.
Generated legal artifacts are evidence, not the human record of approval.
Native runtime file provenance is recorded in
`docs/licensing/native-runtime-provenance.json` and enforced by the legal
artifact generator.

## Adding Dependencies

Before adding a dependency, check:

- Runtime versus development scope.
- License expression and notice requirements.
- Whether native binaries, models, or assets are bundled.
- Whether the dependency changes source availability or binary distribution obligations.

Run the legal artifact generator before release:

```bash
python desktop/scripts/generate_legal_artifacts.py
```

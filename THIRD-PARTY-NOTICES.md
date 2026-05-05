# Third-Party Notices

This file is a public preview placeholder. Generate release notices before distributing source or binary artifacts:

```bash
python desktop/scripts/generate_legal_artifacts.py
```

Generated desktop notices are written under `desktop/dist/legal/`. Release artifacts should include the generated notices, dependency inventory, and any required source availability information.

PyMuPDF/MuPDF requires release review while it remains in the client runtime dependency stack. See `docs/licensing/dependency-policy.md` and `docs/licensing/release-compliance.md`.

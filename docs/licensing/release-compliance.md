# Release Compliance

This checklist applies before distributing ScriptScore source archives or binary desktop artifacts.

## Required For Public Releases

- Include the root `LICENSE`.
- Include `NOTICE` and third-party notices.
- Generate dependency inventory and license policy report.
- Include build instructions for the released source.
- Keep lockfiles available for reproducibility.
- Verify source availability for AGPL obligations.
- Review PyMuPDF/MuPDF obligations while PyMuPDF remains in the runtime stack.
- Confirm bundled desktop legal resources under `desktop/dist/legal/`.

## Recommended Commands

```bash
python desktop/scripts/generate_legal_artifacts.py
python scripts/check_spdx_headers.py
python desktop/scripts/check_scriptscoreplus_boundary.py
```

## Binary Release Caution

Do not publish binary artifacts until third-party notices, SBOM/dependency inventory, source availability, and PyMuPDF/MuPDF obligations have been reviewed for that release.

Linux is the only verified desktop packaging target for the current public preview. Windows and macOS installers are in progress and should not be published until platform-specific testing and release review are complete.

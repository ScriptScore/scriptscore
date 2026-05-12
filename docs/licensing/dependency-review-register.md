# Dependency Review Register

This file is the human document of record for dependency, asset, model, and
native-binary license review decisions in ScriptScore public client releases.

Generated files such as `desktop/dist/legal/license-policy-report.json` and
`desktop/dist/legal/THIRD_PARTY_NOTICES.md` are evidence and packaging inputs.
Do not edit generated legal artifacts to record review decisions. Update this
register, then update generator policy or overrides when a decision should be
enforced mechanically.

## Status Values

- `approved`: acceptable for the stated release scope with listed obligations.
- `normalized`: detected metadata is noisy, but the reviewed license expression
  is recorded and may be normalized by the generator.
- `first_party`: generated from ScriptScore source or assets already governed by
  the repository license or notice files.
- `covered_by_source_package`: generated build output is covered by a reviewed
  source dependency or source asset; do not review the hashed output file alone.
- `pending`: must be reviewed before release.
- `replace_before_release`: remove or replace before distributing the affected
  artifact.
- `excluded`: confirmed absent from the stated release artifact or release
  dependency set.
- `blocked`: do not distribute in the affected release scope.

## How To Close A Finding

For each `license-policy-report.json` finding, record one row below with:

- the package or artifact identity,
- runtime or release scope,
- detected license,
- review status,
- decision rationale,
- release obligations,
- reviewer and review date,
- evidence such as upstream license URLs, package metadata, local `NOTICE`
  entries, source package names, or model provenance.

Rows with `pending`, `replace_before_release`, or `blocked` are not release
approved. The release owner must resolve them before publishing the affected
binary or source artifact.

## Review Records

| Item or Pattern | Scope | Detected License | Status | Decision And Rationale | Release Obligations | Reviewer / Date | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `pandas` | Python runtime | Long wheel `License` prose beginning with BSD 3-Clause text | normalized | Treat the package license expression as `BSD-3-Clause`; preserve long upstream metadata as a license note when present. Native extension files are reviewed through the native runtime provenance manifest. | Include generated third-party notices. | dennisaschmidt / 2026-05-12 | `desktop/scripts/generate_legal_artifacts.py` normalization; upstream package metadata; `docs/licensing/native-runtime-provenance.json`. |
| `scipy` | Python runtime | `BSD-3-Clause AND BSD-3-Clause-Open-MPI AND GPL-3.0-or-later WITH GCC-exception-3.1` | approved | Approve the current SciPy wheel for the AGPL client runtime. The GPL expression is tied to bundled-library/runtime-exception metadata captured from the wheel, not an unresolved incompatible dependency finding for this release posture. Native extension files and bundled OpenBLAS evidence are reviewed through the native runtime provenance manifest. | Include generated notices and bundled-library attributions from the wheel metadata. Reopen review if SciPy version or license metadata changes. | dennisaschmidt / 2026-05-12 | SciPy wheel metadata; generated `license-policy-report.json`; `docs/licensing/native-runtime-provenance.json`. |
| `ujson` | Python runtime | `BSD-3-Clause AND TCL` | approved | Approve the current ujson wheel license expression. `TCL` is accepted as a permissive Tcl-style license for the bundled UltraJSON code path under the current AGPL client posture. | Include generated third-party notices when bundled. Reopen review if package version or license metadata changes. | dennisaschmidt / 2026-05-12 | ujson wheel metadata; generated `license-policy-report.json`; `docs/licensing/native-runtime-provenance.json`. |
| `PyMuPDF` / MuPDF | Python runtime | Dual licensed AGPL-3.0 or commercial | approved | Required by ScriptScore PDF workflows for opening, rendering, text extraction, stamping, and redaction. Approved for the public client only under the current AGPL-3.0-only release posture while PyMuPDF remains bundled. | Include AGPL source offer/source availability, root license, generated notices, and any MuPDF/PyMuPDF-required notices for each binary release. Do not use this approval for proprietary or non-AGPL distribution without Artifex commercial licensing. | dennisaschmidt / 2026-05-11 | `docs/licensing/dependency-policy.md`; PyMuPDF upstream license and package metadata; CLI `fitz` imports. |
| `aistudio-sdk` | Portable Python runtime | Unknown upstream wheel metadata | excluded | Upstream metadata does not provide a usable license expression. The portable runtime builder removes `aistudio-sdk` from the exported lock and installs the remaining locked requirements with dependency resolution disabled so `paddlex` cannot pull it back into the release runtime. | Keep excluded unless upstream publishes usable license/source evidence; fail release review if generated legal artifacts show it in a portable runtime. | Codex / 2026-05-11 | Upstream package metadata; `desktop/scripts/prepare_portable_python.py`; generated `license-policy-report.json`. |
| `opencv-contrib-python` | Portable Python runtime | Apache-2.0 package metadata plus GUI-enabled bundled native libraries | excluded | PaddleX's `ocr-core` extra declares the non-headless OpenCV wheel, but ScriptScore release runtimes use the direct `opencv-contrib-python-headless` dependency and do not require GUI OpenCV APIs. | Do not distribute in bundled desktop runtime artifacts. Portable-runtime preparation removes it, generated legal policy treats it as blocked if present, release package verification rejects it if installed, and the RC workflow enforces absence from the generated Python SBOM. | Codex / 2026-05-11 | `cli/pyproject.toml`; `cli/uv.lock`; `desktop/scripts/prepare_portable_python.py`; `desktop/scripts/generate_legal_artifacts.py`; `desktop/scripts/verify-release-package-artifacts.py`; `desktop/scripts/enforce_release_legal_policy.py`. |
| `python-bidi` | Portable Python runtime | GNU Library or Lesser General Public License (LGPL) | approved | Required by the current PaddleOCR/PaddleX OCR dependency set and also used by the non-macOS EasyOCR dependency path. Approved as an LGPL-family runtime exception for ScriptScore's bundled OCR runtime. | Include LGPL license text, package notices, source availability/source-offer information, and relink/replacement compliance notes for the released artifact. Reopen review if package version or license metadata changes. | dennisaschmidt / 2026-05-11 | `cli/uv.lock`; `desktop/scripts/generate_legal_artifacts.py`; PaddleX `ocr-core` metadata; package metadata. |
| `bce-python-sdk`, `crc32c` | Portable Python runtime | Apache-2.0 / LGPL-2.1-or-later | excluded | `bce-python-sdk` is pulled through PaddleX/aistudio cloud paths rather than ScriptScore's offline OCR flow, and `crc32c` is only required by `bce-python-sdk`. Current portable-runtime preparation excludes both, the bundled runtime smoke validates without them, and release verification fails if either reappears. | Do not distribute in bundled desktop runtime artifacts. Portable-runtime preparation removes both, generated legal policy treats both as blocked if present, and release package verification rejects either if installed. | dennisaschmidt / 2026-05-12 | `cli/uv.lock`; `uv tree --package scriptscore`; installed runtime metadata; `desktop/scripts/prepare_portable_python.py`; `desktop/scripts/generate_legal_artifacts.py`; `desktop/scripts/verify-release-package-artifacts.py`; `desktop/scripts/enforce_release_legal_policy.py`; RC workflow 25710290453. |
| `defusedxml` | Python runtime | `PSFL` | normalized | Treat `PSFL` metadata as `PSF-2.0`, the SPDX identifier for Python Software Foundation License 2.0; accepted as permissive for the current AGPL client posture. | Include generated third-party notices when bundled. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; PyPI metadata; SPDX `PSF-2.0`. |
| `pillow` | Python runtime | `MIT-CMU` | approved | `MIT-CMU` is a recognized permissive SPDX license identifier and is accepted by the generator policy. | Include generated third-party notices when bundled. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; SPDX `MIT-CMU`; Pillow license metadata. |
| `python-dateutil` | Python runtime | `Dual License` | normalized | Normalize ambiguous wheel metadata to `BSD-3-Clause OR Apache-2.0`, matching upstream's documented dual-license history. | Include generated third-party notices when bundled. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; python-dateutil PyPI license text; SPDX expression policy. |
| `safetensors`, `sortedcontainers` | Python runtime | Apache-family metadata such as `Apache 2.0` or `Apache Software License` | normalized | Normalize Apache-family metadata aliases to `Apache-2.0`, which is already accepted by policy. | Include generated third-party notices when bundled. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; package metadata; Apache-2.0 SPDX identifier. |
| `typing_extensions` | Python runtime | `PSF-2.0` | approved | `PSF-2.0` is accepted as a permissive Python Software Foundation license expression for the current AGPL client posture. | Include generated third-party notices when bundled. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; package metadata; SPDX `PSF-2.0`. |
| `lru-cache`, `minimatch` | npm dependency / build tooling | `BlueOak-1.0.0` | approved | `BlueOak-1.0.0` is accepted as a permissive license for dev/build tooling and any source-package distribution that preserves license text or links. | Retain Blue Oak license text or link when distributing affected package copies. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; npm package metadata; SPDX `BlueOak-1.0.0`. |
| `r-efi` 5.3.0 and 6.0.0 | Cargo runtime | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | approved | The SPDX `OR` expression offers permissive MIT or Apache-2.0 alternatives; the generator may choose an approved permissive branch without treating the LGPL alternative as mandatory. | Keep the license expression in the SBOM and include generated third-party notices when bundled. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; Cargo metadata; SPDX license-expression semantics. |
| `webpki-roots` | Cargo runtime data | `CDLA-Permissive-2.0` | approved | `CDLA-Permissive-2.0` is accepted for Mozilla root certificate data with data-license notice obligations. | Include CDLA-Permissive-2.0 text and retain data provenance when bundled. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; Cargo metadata; SPDX `CDLA-Permissive-2.0`. |
| `astroid`, `pylint` | Portable Python runtime | LGPL / GPL development-tool metadata | excluded | These packages are development quality tooling only. They are outside `[project].dependencies`, absent from `uv export --no-dev`, and guarded by portable-runtime preparation plus release-package verification. | Do not distribute in bundled desktop runtime artifacts. If either appears in runtime requirements, installed portable Python metadata, or generated legal runtime SBOMs, fail release verification and remove the dependency path. | Codex / 2026-05-11 | `cli/pyproject.toml`; `cli/uv.lock`; `desktop/scripts/prepare_portable_python.py`; `desktop/scripts/verify-release-package-artifacts.py`; generated `sbom-python.json`. |
| `pip-api`, `pip-audit` | Portable Python runtime | Apache-2.0-family security-tool metadata | excluded | These packages are development/security tooling only. They are outside `[project].dependencies`, absent from `uv export --no-dev`, and guarded by portable-runtime preparation plus release-package verification. | Do not distribute in bundled desktop runtime artifacts. If either appears in runtime requirements, installed portable Python metadata, or generated legal runtime SBOMs, fail release verification and remove the dependency path. | Codex / 2026-05-11 | `cli/pyproject.toml`; `cli/uv.lock`; `desktop/scripts/prepare_portable_python.py`; `desktop/scripts/verify-release-package-artifacts.py`; generated `sbom-python.json`. |
| Generated frontend JS/CSS/HTML/JSON build outputs under `desktop/frontend/build/_app/` plus `desktop/frontend/build/index.html` | Frontend build output | none on generated files | covered_by_source_package | Review source npm packages and first-party frontend source, not each hashed output file. Generated files should not appear as unresolved third-party assets once source-package coverage is mapped. | Include npm SBOM and generated notices for third-party source packages. | dennisaschmidt / 2026-05-12 | `package-lock.json`; generated npm SBOM; frontend source tree. |
| Figtree and Inter WOFF2 files under `desktop/frontend/build/_app/immutable/assets/` | Frontend font asset | `OFL-1.1` via Fontsource packages | approved | Map emitted hashed WOFF2 files back to `@fontsource-variable/figtree` and `@fontsource-variable/inter`; both source packages declare `OFL-1.1` and include license files in `desktop/frontend/node_modules`. | Include the generated font-asset SBOM entries and license-note evidence for the source packages. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; `desktop/frontend/package-lock.json`; `desktop/frontend/node_modules/@fontsource-variable/*/LICENSE`; generated `sbom-assets.json` and `THIRD_PARTY_NOTICES.md`. |
| App images and icons under `desktop/frontend/build/` | Frontend static asset | `AGPL-3.0-only` via first-party provenance manifest | first_party | The listed static frontend guide images, app icon, and mark are first-party ScriptScore assets committed under `desktop/frontend/static/`; generated build copies are mapped back to those source assets by the legal generator. | Retain the source assets, root `LICENSE`/`NOTICE`, and generated provenance notes in release legal artifacts. | dennisaschmidt / 2026-05-11 | `docs/licensing/frontend-static-asset-provenance.json`; `desktop/scripts/generate_legal_artifacts.py`; generated `sbom-assets.json` and `THIRD_PARTY_NOTICES.md`. |
| Bundled Paddle OCR model files under `cli/models/paddle/` | Model asset | `Apache-2.0` via PaddleOCR model provenance manifest | approved | The bundled files match the official `PaddlePaddle/PP-OCRv5_mobile_det` and `PaddlePaddle/PP-OCRv5_mobile_rec` Hugging Face model repositories at pinned commits. Local SHA-256 checksums are recorded for each bundled file and the Paddle model pages declare `apache-2.0`. | Include generated model-asset SBOM entries, Apache-2.0 license/provenance notes, source URLs, and redistribution terms in release legal artifacts. Re-check checksums before replacing model files. | dennisaschmidt / 2026-05-11 | `docs/licensing/paddle-ocr-model-provenance.json`; PaddlePaddle Hugging Face model pages; PaddleOCR Apache-2.0 license; generated `sbom-assets.json` and `THIRD_PARTY_NOTICES.md`. |
| Native runtime libraries discovered under bundled runtime | Native binary | varies by source package | approved | Current native runtime files are mapped by path pattern to reviewed source packages in the native runtime provenance manifest. Unmatched future native files remain `review_required` and fail release legal checks. | Include generated native SBOM entries, package notices, bundled-library attributions, and source links where applicable. Reopen review when native package versions or bundled runtime contents change. | dennisaschmidt / 2026-05-12 | `docs/licensing/native-runtime-provenance.json`; runtime manifest; generated assets/native SBOM; generated `THIRD_PARTY_NOTICES.md`. |

## Current RC Closure Notes

The generated `desktop/dist/legal/license-policy-report.json` for the current
local RC evidence has zero unresolved findings after applying the review records
above and the native runtime provenance manifest. These notes summarize the
durable release controls that should remain in place.

| Area | Current Decision | Continuing Control |
| --- | --- | --- |
| Excluded runtime packages | `aistudio-sdk`, `bce-python-sdk`, `crc32c`, `opencv-contrib-python`, `astroid`, `pylint`, `pip-api`, and `pip-audit` are excluded from distributed portable runtimes. | Portable-runtime preparation removes them, release package verification rejects them if installed, and release legal policy fails if they appear in runtime SBOM evidence. |
| Native runtime libraries | Current native binaries are approved through `docs/licensing/native-runtime-provenance.json`, including CPython/Tcl runtime libraries and native files from SciPy, NumPy, pandas, scikit-image, PyCryptodome, Torch/TorchVision, PaddlePaddle, OpenCV headless, Pillow, Shapely, PDFium, ujson, PyYAML, protobuf, hf-xet, MarkupSafe, psutil, pyclipper, pydantic-core, safetensors, charset-normalizer, and chardet. | Unmatched native binaries remain `review_required` and fail generator check mode plus RC release policy enforcement. |
| PyMuPDF/MuPDF | Approved only under the current AGPL-3.0-only public client posture. | Keep source availability/source-offer evidence and generated notices current for every binary release; do not reuse this approval for proprietary or non-AGPL distribution. |
| Frontend assets and Paddle OCR models | First-party frontend static assets, Fontsource fonts, and Paddle OCR model files are resolved through their provenance manifests. | Regenerate legal artifacts after build/model changes and reopen review if checksums, source packages, or upstream license evidence change. |

## Release Use

Before publishing a source archive or desktop binary:

1. Run `python desktop/scripts/generate_legal_artifacts.py`.
2. Open `desktop/dist/legal/license-policy-report.json`.
3. For each finding, update or verify the matching row in this register.
4. If the decision is durable and mechanical, update generator normalization or
   policy checks so the same issue does not reappear as unresolved.
5. Do not publish while any release-scope row remains `pending`,
   `replace_before_release`, or `blocked`.

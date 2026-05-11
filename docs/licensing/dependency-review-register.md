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
| `pandas` | Python runtime | Long wheel `License` prose beginning with BSD 3-Clause text | normalized | Treat the package license expression as `BSD-3-Clause`; preserve long upstream metadata as a license note when present. | Include generated third-party notices. | pending reviewer / pending date | `desktop/scripts/generate_legal_artifacts.py` normalization; upstream package metadata. |
| `scipy` | Python runtime | BSD/GPL exception metadata | pending | Confirm effective SciPy and bundled-library obligations for the released runtime. Current generator keeps this in review. | Include notices and any required bundled-library attributions; resolve before binary release. | pending reviewer / pending date | SciPy wheel metadata; generated `license-policy-report.json`. |
| `PyMuPDF` / MuPDF | Python runtime | Dual licensed AGPL-3.0 or commercial | approved | Required by ScriptScore PDF workflows for opening, rendering, text extraction, stamping, and redaction. Approved for the public client only under the current AGPL-3.0-only release posture while PyMuPDF remains bundled. | Include AGPL source offer/source availability, root license, generated notices, and any MuPDF/PyMuPDF-required notices for each binary release. Do not use this approval for proprietary or non-AGPL distribution without Artifex commercial licensing. | dennisaschmidt / 2026-05-11 | `docs/licensing/dependency-policy.md`; PyMuPDF upstream license and package metadata; CLI `fitz` imports. |
| `aistudio-sdk` | Portable Python runtime | Unknown upstream wheel metadata | excluded | Upstream metadata does not provide a usable license expression. The portable runtime builder removes `aistudio-sdk` from the exported lock and installs the remaining locked requirements with dependency resolution disabled so `paddlex` cannot pull it back into the release runtime. | Keep excluded unless upstream publishes usable license/source evidence; fail release review if generated legal artifacts show it in a portable runtime. | Codex / 2026-05-11 | Upstream package metadata; `desktop/scripts/prepare_portable_python.py`; generated `license-policy-report.json`. |
| `opencv-contrib-python` | Portable Python runtime | Apache-2.0 package metadata plus GUI-enabled bundled native libraries | excluded | PaddleX's `ocr-core` extra declares the non-headless OpenCV wheel, but ScriptScore release runtimes use the direct `opencv-contrib-python-headless` dependency and do not require GUI OpenCV APIs. | Do not distribute in bundled desktop runtime artifacts. Portable-runtime preparation removes it, generated legal policy treats it as blocked if present, release package verification rejects it if installed, and the RC workflow enforces absence from the generated Python SBOM. | Codex / 2026-05-11 | `cli/pyproject.toml`; `cli/uv.lock`; `desktop/scripts/prepare_portable_python.py`; `desktop/scripts/generate_legal_artifacts.py`; `desktop/scripts/verify-release-package-artifacts.py`; `desktop/scripts/enforce_release_legal_policy.py`. |
| `python-bidi` | Portable Python runtime | GNU Library or Lesser General Public License (LGPL) | approved | Required by the current PaddleOCR/PaddleX OCR dependency set and also used by the non-macOS EasyOCR dependency path. Approved as an LGPL-family runtime exception for ScriptScore's bundled OCR runtime. | Include LGPL license text, package notices, source availability/source-offer information, and relink/replacement compliance notes for the released artifact. Reopen review if package version or license metadata changes. | dennisaschmidt / 2026-05-11 | `cli/uv.lock`; `desktop/scripts/generate_legal_artifacts.py`; PaddleX `ocr-core` metadata; package metadata. |
| `bce-python-sdk`, `crc32c` | Portable Python runtime | Apache-2.0 / LGPL-2.1-or-later | excluded | Removal testing is underway. `bce-python-sdk` is pulled through PaddleX/aistudio cloud paths rather than ScriptScore's offline OCR flow, and `crc32c` is only required by `bce-python-sdk`. | Do not distribute in bundled desktop runtime artifacts if PaddleOCR/PII smoke and RC runtime validation pass without them. Portable-runtime preparation removes both, generated legal policy treats both as blocked if present, and release package verification rejects either if installed. | Codex / 2026-05-11 | `cli/uv.lock`; `uv tree --package scriptscore`; installed runtime metadata; `desktop/scripts/prepare_portable_python.py`; `desktop/scripts/generate_legal_artifacts.py`; `desktop/scripts/verify-release-package-artifacts.py`; `desktop/scripts/enforce_release_legal_policy.py`. |
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
| Generated frontend JS/CSS/HTML/JSON build outputs under `desktop/frontend/build/_app/` plus `desktop/frontend/build/index.html` | Frontend build output | none on generated files | covered_by_source_package | Review source npm packages and first-party frontend source, not each hashed output file. Generated files should not appear as unresolved third-party assets once source-package coverage is mapped. | Include npm SBOM and generated notices for third-party source packages. | pending reviewer / pending date | `package-lock.json`; generated npm SBOM; frontend source tree. |
| Figtree and Inter WOFF2 files under `desktop/frontend/build/_app/immutable/assets/` | Frontend font asset | `OFL-1.1` via Fontsource packages | approved | Map emitted hashed WOFF2 files back to `@fontsource-variable/figtree` and `@fontsource-variable/inter`; both source packages declare `OFL-1.1` and include license files in `desktop/frontend/node_modules`. | Include the generated font-asset SBOM entries and license-note evidence for the source packages. | dennisaschmidt / 2026-05-11 | `desktop/scripts/generate_legal_artifacts.py`; `desktop/frontend/package-lock.json`; `desktop/frontend/node_modules/@fontsource-variable/*/LICENSE`; generated `sbom-assets.json` and `THIRD_PARTY_NOTICES.md`. |
| App images and icons under `desktop/frontend/build/` | Frontend static asset | `AGPL-3.0-only` via first-party provenance manifest | first_party | The listed static frontend guide images, app icon, and mark are first-party ScriptScore assets committed under `desktop/frontend/static/`; generated build copies are mapped back to those source assets by the legal generator. | Retain the source assets, root `LICENSE`/`NOTICE`, and generated provenance notes in release legal artifacts. | dennisaschmidt / 2026-05-11 | `docs/licensing/frontend-static-asset-provenance.json`; `desktop/scripts/generate_legal_artifacts.py`; generated `sbom-assets.json` and `THIRD_PARTY_NOTICES.md`. |
| Bundled Paddle OCR model files under `cli/models/paddle/` | Model asset | none on local files | pending | Confirm model source, license, redistribution terms, and required notices before bundling in a public binary release. | Include model license/provenance notices or stop bundling models. | pending reviewer / pending date | PaddleOCR/Paddle model source and license documentation. |
| Native runtime libraries discovered under bundled runtime | Native binary | varies or missing | pending | Confirm native library provenance, license, and redistribution obligations before binary release. | Include required notices and source links where applicable. | pending reviewer / pending date | Runtime manifest; generated assets/native SBOM. |

## Preliminary Judgements For Current Report

These judgements are an engineering compliance triage of the current
`desktop/dist/legal/license-policy-report.json`, not final legal approval.
The current report has no `blocked` or `unknown` runtime failures, but it still
has `review_required` findings that should be resolved or documented before a
binary release.

| Finding(s) | Preliminary Judgement | Action Before Release |
| --- | --- | --- |
| `aistudio-sdk` | Exclude from the distributed portable runtime unless upstream license evidence is obtained. PyPI currently provides no usable license metadata or source distribution for the reported wheel. | `desktop/scripts/prepare_portable_python.py` filters it from the exported lock and installs with `--no-deps` so `paddlex` does not re-resolve it. Revisit only if PaddleOCR/PaddleX starts importing it at runtime or upstream publishes authoritative license/source evidence. |
| `astroid`, `pylint` | Excluded from the portable desktop runtime. `pylint` is GPL and `astroid` is LGPL, but both are development quality tooling and are now mechanically blocked from bundled runtime artifacts. | Keep portable-runtime preparation, generated legal artifact checks, and release-package verification failing if either package appears in release runtime evidence. No product notice needed while not distributed. |
| `crc32c` | Exclusion attempt in progress. Current evidence shows no ScriptScore direct import; it is required only by `bce-python-sdk`, which appears tied to excluded PaddleX/aistudio cloud paths rather than offline OCR. | Exclude `bce-python-sdk` and `crc32c` from portable-runtime preparation, rebuild the runtime, run PaddleOCR/PII smoke, and keep release verification failing if either package appears. If OCR validation fails, revert the exclusion path and resolve LGPL/source-notice obligations instead. |
| `defusedxml` | Resolved as `normalized`. `PSFL` maps to the accepted `PSF-2.0` expression. | Verify generated notices include the package when bundled. |
| `opencv-contrib-python-headless` | Likely acceptable with obligations. Metadata is Apache-2.0, but OpenCV wheels include bundled third-party binaries and notices. | Prefer the headless wheel for release. Include OpenCV and bundled third-party notices, including FFmpeg/LGPL notes where applicable. |
| `opencv-contrib-python` | Excluded from the portable desktop runtime. The non-headless wheel can bring GUI dependencies such as Qt with additional LGPL obligations, and current ScriptScore/PaddleOCR usage does not require GUI OpenCV APIs. | Keep portable-runtime preparation, generated legal policy, release-package verification, and RC legal enforcement failing if the non-headless package appears in release runtime evidence. |
| `paddlepaddle` | Likely acceptable with obligations. PyPI identifies PaddlePaddle as Apache-2.0, but the wheel is a large native runtime with bundled libraries. | Keep pending until binary wheel contents and notices are captured in the release notice/SBOM. |
| `pillow` | Resolved as `approved`. `MIT-CMU` is accepted by policy. | Verify generated notices include the package when bundled. |
| `pip-api`, `pip_audit` | Excluded from the portable desktop runtime. Both are Apache-2.0-family security/development tooling findings and are now mechanically blocked from bundled runtime artifacts. | Keep portable-runtime preparation, generated legal artifact checks, and release-package verification failing if either package appears in release runtime evidence. If intentionally bundled later, record a new approval and include Apache-2.0 notices. |
| `PyMuPDF` / MuPDF | Approved only under the AGPL release posture, with explicit obligations. Not acceptable for proprietary or non-AGPL distribution without commercial licensing from Artifex. | Keep generated notices and source-offer/source-availability evidence current for every binary release; reopen review if package version or license metadata changes. |
| `python-bidi` | Approved as an LGPL-family OCR runtime exception for the current dependency set. | Include LGPL text, package notices, source availability/source-offer information, and relink/replacement compliance notes; reopen review if package version or license metadata changes. |
| `python-dateutil` | Resolved as `normalized`. `Dual License` metadata maps to `BSD-3-Clause OR Apache-2.0`. | Verify generated notices include the package when bundled. |
| `safetensors`, `sortedcontainers` | Resolved as `normalized`. Apache-family metadata aliases map to `Apache-2.0`. | Verify generated notices include the packages when bundled. |
| `typing_extensions` | Resolved as `approved`. `PSF-2.0` is accepted by policy. | Verify generated notices include the package when bundled. |
| `eslint-plugin-sonarjs` | Removed from the enforced frontend ESLint dependency path before release-quality source publication. Package metadata said `LGPL-3.0-only`, but the installed package also contained a Sonar Source-Available license with field-of-use restrictions. | Keep Sonar analysis external to the release dependency tree, for example as a separately reviewed CI service/tool if needed. Do not bundle or vendor this package. |
| `lru-cache`, `minimatch` | Resolved as `approved` for dev/build tooling. `BlueOak-1.0.0` is accepted by policy. | Retain Blue Oak license text or link when distributing affected package copies. |
| `r-efi` 5.3.0 and 6.0.0 | Resolved as `approved`. The generator now treats `OR` expressions as license choices and accepts the MIT/Apache alternatives. | Keep the full expression in the SBOM and generated notices when bundled. |
| `webpki-roots` | Resolved as `approved` with data-license obligations. `CDLA-Permissive-2.0` is accepted by policy. | Include CDLA-Permissive-2.0 text and retain data provenance when bundled. |
| Figtree font WOFF2 files under `desktop/frontend/build/_app/immutable/assets/` | Resolved as `approved`. Generated legal artifacts map emitted Figtree WOFF2 files to `@fontsource-variable/figtree` with `OFL-1.1` package evidence. | Verify generated `sbom-assets.json` and `THIRD_PARTY_NOTICES.md` include the mapped font asset entries when build output is regenerated. |
| Inter font WOFF2 files under `desktop/frontend/build/_app/immutable/assets/` | Resolved as `approved`. Generated legal artifacts map emitted Inter WOFF2 files to `@fontsource-variable/inter` with `OFL-1.1` package evidence. | Verify generated `sbom-assets.json` and `THIRD_PARTY_NOTICES.md` include the mapped font asset entries when build output is regenerated. |
| `alignment-marks-infographic.png`, `canvas-api-token-guide.png`, `ollama-api-token-guide.png`, `redaction-regions-infographic.png`, `student-intake-guide.png` | Resolved as `first_party`. The static asset provenance manifest records these as first-party ScriptScore UI guide images under `AGPL-3.0-only`, and the legal generator maps build copies back to the source files. | Verify generated `sbom-assets.json` and `THIRD_PARTY_NOTICES.md` include `frontend-static-asset` entries when build output is regenerated. |
| `scriptscore-app-icon.png`, `scriptscore-mark.svg` | Resolved as `first_party`. The static asset provenance manifest records the app icon and mark as first-party ScriptScore assets under `AGPL-3.0-only`, and the legal generator maps build copies back to the source files. | Verify generated `sbom-assets.json` and `THIRD_PARTY_NOTICES.md` include `frontend-static-asset` entries when build output is regenerated. |
| Paddle OCR det/rec model files under `cli/models/paddle/` | Likely acceptable if they are the official PP-OCRv5 mobile models under Apache-2.0, but still pending because local files lack embedded provenance/license metadata. | Record exact source URLs, model names, checksums, license, and required notices. If provenance cannot be established, stop bundling them. |

## Release Use

Before publishing a source archive or desktop binary:

1. Run `python desktop/scripts/generate_legal_artifacts.py`.
2. Open `desktop/dist/legal/license-policy-report.json`.
3. For each finding, update or verify the matching row in this register.
4. If the decision is durable and mechanical, update generator normalization or
   policy checks so the same issue does not reappear as unresolved.
5. Do not publish while any release-scope row remains `pending`,
   `replace_before_release`, or `blocked`.

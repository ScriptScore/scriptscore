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
| `PyMuPDF` / MuPDF | Python runtime | Dual licensed AGPL-3.0 or commercial | pending | Current public preview posture is AGPL-3.0-only while PyMuPDF remains in the distributed client runtime. Confirm release obligations for each binary release. | Include AGPL source offer, root license, notices, and any MuPDF/PyMuPDF-required notices. | pending reviewer / pending date | `docs/licensing/dependency-policy.md`; PyMuPDF upstream license. |
| `aistudio-sdk` | Portable Python runtime | Unknown upstream wheel metadata | excluded | Upstream metadata does not provide a usable license expression. The portable runtime builder removes `aistudio-sdk` from the exported lock and installs the remaining locked requirements with dependency resolution disabled so `paddlex` cannot pull it back into the release runtime. | Keep excluded unless upstream publishes usable license/source evidence; fail release review if generated legal artifacts show it in a portable runtime. | Codex / 2026-05-11 | Upstream package metadata; `desktop/scripts/prepare_portable_python.py`; generated `license-policy-report.json`. |
| Generated frontend JS/CSS/HTML/JSON build outputs under `desktop/frontend/build/_app/` plus `desktop/frontend/build/index.html` | Frontend build output | none on generated files | covered_by_source_package | Review source npm packages and first-party frontend source, not each hashed output file. Generated files should not appear as unresolved third-party assets once source-package coverage is mapped. | Include npm SBOM and generated notices for third-party source packages. | pending reviewer / pending date | `package-lock.json`; generated npm SBOM; frontend source tree. |
| App images and icons under `desktop/frontend/build/` | Frontend asset | none on generated files | pending | Confirm whether each asset is first-party, generated for ScriptScore, or third-party. Record source/provenance before release. | Include attribution/notice if third-party; otherwise document first-party status. | pending reviewer / pending date | Source asset files and creation/provenance notes. |
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
| `astroid`, `pylint` | Acceptable for developer tooling only; not approved for bundled runtime. `pylint` is GPL and `astroid` is LGPL, but both are from the development quality toolchain. | Ensure the portable desktop runtime excludes dev/quality dependencies. No product notice needed if not distributed. |
| `crc32c` | Pending for bundled runtime. LGPL-2.1-or-later plus embedded algorithm notices may be acceptable, but only with LGPL/source-notice obligations satisfied. | Confirm whether it is actually bundled. If bundled, include license text, notices, and source/relink compliance notes, or remove the dependency path. |
| `defusedxml` | Normalize/approve. PSFL is a permissive license compatible with the current policy posture. | Add `PSFL` to allowed tokens or normalize metadata to the SPDX expression used by the policy. Include notice when bundled. |
| `opencv-contrib-python-headless` | Likely acceptable with obligations. Metadata is Apache-2.0, but OpenCV wheels include bundled third-party binaries and notices. | Prefer the headless wheel for release. Include OpenCV and bundled third-party notices, including FFmpeg/LGPL notes where applicable. |
| `opencv-contrib-python` | Do not approve for release by default. The non-headless wheel can bring GUI dependencies such as Qt with additional LGPL obligations. | Avoid bundling the non-headless package unless there is a specific need and its bundled-library notice set is reviewed. |
| `paddlepaddle` | Likely acceptable with obligations. PyPI identifies PaddlePaddle as Apache-2.0, but the wheel is a large native runtime with bundled libraries. | Keep pending until binary wheel contents and notices are captured in the release notice/SBOM. |
| `pillow` | Normalize/approve. `MIT-CMU` is permissive. | Add `MIT-CMU` to allowed tokens or normalize it to a policy-approved permissive expression; include notice when bundled. |
| `pip-api`, `pip_audit` | Acceptable for security/development tooling only. Both are Apache-2.0-family metadata findings and should not ship in the product runtime. | Ensure the portable runtime excludes dev/security-audit dependencies. If intentionally bundled, include Apache-2.0 notices. |
| `PyMuPDF` / MuPDF | Acceptable only under the AGPL release posture, with explicit obligations. Not acceptable for proprietary or non-AGPL distribution without commercial licensing from Artifex. | Keep pending until the release checklist confirms AGPL source offer, MuPDF/PyMuPDF notices, and any required source availability. |
| `python-bidi` | Pending for bundled runtime. LGPL is not automatically disqualifying, but it requires notice/source compliance. | Confirm whether bundled via PaddleOCR/PaddleX. If bundled, include LGPL text and source/relink compliance notes, or remove the path. |
| `python-dateutil` | Normalize/approve. The project documents BSD-3-Clause and Apache-2.0 dual-license history. | Normalize `Dual License` metadata to `BSD-3-Clause OR Apache-2.0`; include notice when bundled. |
| `safetensors`, `sortedcontainers` | Normalize/approve. Both report Apache-2.0-family licensing. | Add normalizations for `Apache Software License` and `Apache 2.0` where needed; include notices when bundled. |
| `typing_extensions` | Normalize/approve. `PSF-2.0` is permissive and already an SPDX expression. | Add `PSF-2.0` to allowed tokens; include notice when bundled. |
| `eslint-plugin-sonarjs` | Removed from the enforced frontend ESLint dependency path before release-quality source publication. Package metadata said `LGPL-3.0-only`, but the installed package also contained a Sonar Source-Available license with field-of-use restrictions. | Keep Sonar analysis external to the release dependency tree, for example as a separately reviewed CI service/tool if needed. Do not bundle or vendor this package. |
| `lru-cache`, `minimatch` | Normalize/approve for dev/build tooling. Current installed packages use `BlueOak-1.0.0`, a permissive model license requiring license text or link with copies. | Add `BlueOak-1.0.0` to allowed tokens if accepted by policy; include license text/link if distributed. |
| `r-efi` 5.3.0 and 6.0.0 | Normalize/approve by choosing a permissive branch of the OR expression, e.g. `MIT` or `Apache-2.0`. | Update policy handling for disjunctive licenses so the LGPL alternative does not force review when MIT/Apache is available. |
| `webpki-roots` | Likely acceptable with data-license obligations. `CDLA-Permissive-2.0` is permissive, but it covers Mozilla root certificate data rather than ordinary code. | Add `CDLA-Permissive-2.0` to allowed tokens if accepted; include license text and retain data provenance. |
| Figtree font WOFF2 files under `desktop/frontend/build/_app/immutable/assets/` | Approve as font assets once mapped back to `@fontsource-variable/figtree`; source package license is OFL-1.1. | Teach the generator to map emitted Figtree font files to the npm package/license and include the OFL text. |
| Inter font WOFF2 files under `desktop/frontend/build/_app/immutable/assets/` | Approve as font assets once mapped back to `@fontsource-variable/inter`; source package license is OFL-1.1. | Teach the generator to map emitted Inter font files to the npm package/license and include the OFL text. |
| `alignment-marks-infographic.png`, `canvas-api-token-guide.png`, `ollama-api-token-guide.png`, `redaction-regions-infographic.png`, `student-intake-guide.png` | Pending provenance. These appear to be first-party static UI guide images, but the repo does not yet record their creation/source status. | Add an asset provenance manifest or NOTICE entry identifying them as first-party/generated-for-ScriptScore, or record third-party source and license if applicable. |
| `scriptscore-app-icon.png`, `scriptscore-mark.svg` | Likely first-party, but still pending documentation. The SVG source is present in `desktop/frontend/static/`; the PNG appears to be the app icon rendering. | Record first-party status and generation relationship, or add source attribution if any third-party source was used. |
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

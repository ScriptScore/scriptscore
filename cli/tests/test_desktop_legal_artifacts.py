# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from types import ModuleType

PROJECT_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = PROJECT_ROOT / "desktop" / "scripts" / "generate_legal_artifacts.py"
GUARD_PATH = PROJECT_ROOT / "desktop" / "scripts" / "check_scriptscoreplus_boundary.py"


def load_module(path: Path, name: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


legal = load_module(SCRIPT_PATH, "generate_legal_artifacts")
guard = load_module(GUARD_PATH, "check_scriptscoreplus_boundary")


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def test_license_policy_allows_permissive_and_flags_unknown_runtime() -> None:
    allowed = legal.InventoryItem(
        name="allowed",
        version="1",
        license="MIT OR Apache-2.0",
        source="npm",
        scope="npm-runtime",
        runtime=True,
    )
    unknown = legal.InventoryItem(
        name="unknown",
        version="1",
        license=None,
        source="python",
        scope="python-runtime",
        runtime=True,
    )
    model = legal.InventoryItem(
        name="cli/models/paddle/det/inference.pdiparams",
        version=None,
        license=None,
        source="assets",
        scope="model-asset",
        runtime=True,
    )
    build_output = legal.InventoryItem(
        name="desktop/frontend/build/_app/immutable/assets/2.CNLA_Ghp.css",
        version=None,
        license=None,
        source="assets",
        scope="frontend-build-output",
        runtime=True,
    )

    assert legal.classify_item(allowed) is None
    assert legal.classify_item(unknown).severity == "unknown"
    assert legal.classify_item(build_output) is None
    model_finding = legal.classify_item(model)
    assert model_finding.severity == "review_required"
    assert "Asset or native binary" in model_finding.message


def test_license_policy_normalizes_approved_permissive_metadata() -> None:
    assert legal.normalize_license("PSFL") == "PSF-2.0"
    assert legal.normalize_license("Python Software Foundation License (PSFL)") == "PSF-2.0"
    assert legal.normalize_license("Apache Software License") == "Apache-2.0"
    assert legal.normalize_license("Apache 2.0") == "Apache-2.0"

    for license_value in (
        "PSF-2.0",
        "MIT-CMU",
        "BlueOak-1.0.0",
        "CDLA-Permissive-2.0",
    ):
        item = legal.InventoryItem(
            name=f"package-{license_value}",
            version="1",
            license=license_value,
            source="python",
            scope="python-runtime",
            runtime=True,
        )
        assert legal.classify_item(item) is None


def test_license_policy_accepts_permissive_or_branch_without_accepting_lgpl_and() -> None:
    r_efi = legal.InventoryItem(
        name="r-efi",
        version="6.0.0",
        license="MIT OR Apache-2.0 OR LGPL-2.1-or-later",
        source="cargo",
        scope="cargo-runtime",
        runtime=True,
    )
    and_expression = legal.InventoryItem(
        name="and-expression",
        version="1",
        license="MIT AND LGPL-2.1-or-later",
        source="cargo",
        scope="cargo-runtime",
        runtime=True,
    )
    gpl_exception = legal.InventoryItem(
        name="gpl-exception",
        version="1",
        license="BSD-3-Clause AND GPL-3.0-or-later WITH GCC-exception-3.1",
        source="python",
        scope="python-runtime",
        runtime=True,
    )

    assert legal.classify_item(r_efi) is None
    assert legal.classify_item(and_expression).severity == "review_required"
    assert legal.classify_item(gpl_exception).severity == "review_required"


def test_python_license_replacements_normalize_or_keep_release_review() -> None:
    dateutil_license, dateutil_notice = legal.python_license_metadata(
        {"name": "python-dateutil", "version": "2.9.0.post0", "license": "Dual License"}
    )
    dateutil = legal.InventoryItem(
        name="python-dateutil",
        version="2.9.0.post0",
        license=dateutil_license,
        source="python",
        scope="python-runtime",
        runtime=True,
        notice=dateutil_notice,
    )

    assert dateutil_notice
    assert dateutil_license == "BSD-3-Clause OR Apache-2.0"
    assert legal.classify_item(dateutil) is None

    pandas_license, pandas_notice = legal.python_license_metadata(
        {
            "name": "pandas",
            "version": "3.0.2",
            "license": "BSD 3-Clause License Copyright text that continues with bundled notices.",
        }
    )
    assert pandas_notice
    assert pandas_license == "BSD-3-Clause"

    scipy_license, scipy_notice = legal.python_license_metadata(
        {
            "name": "scipy",
            "version": "1.17.1",
            "license": "BSD-3-Clause plus GPL exception prose mentioning NonCommercial",
        }
    )
    scipy = legal.InventoryItem(
        name="scipy",
        version="1.17.1",
        license=scipy_license,
        source="python",
        scope="python-runtime",
        runtime=True,
        notice=scipy_notice,
    )
    scipy_finding = legal.classify_item(scipy)

    assert scipy_notice
    assert "GCC-exception-3.1" in scipy_license
    assert scipy_finding.severity == "review_required"
    assert "GPL/LGPL" in scipy_finding.message

    aistudio_license, aistudio_notice = legal.python_license_metadata(
        {"name": "aistudio-sdk", "version": "0.3.8", "license": "UNKNOWN"}
    )
    aistudio = legal.InventoryItem(
        name="aistudio-sdk",
        version="0.3.8",
        license=aistudio_license,
        source="python",
        scope="python-runtime",
        runtime=True,
        notice=aistudio_notice,
    )
    aistudio_finding = legal.classify_item(aistudio)

    assert aistudio_notice
    assert aistudio_license == "LicenseRef-REVIEW-aistudio-sdk"
    assert aistudio_finding.severity == "blocked"
    assert "must not appear in distributed runtime" in aistudio_finding.message

    pylint = legal.InventoryItem(
        name="pylint",
        version="3.3.9",
        license="GPL-2.0-or-later",
        source="python",
        scope="python-runtime",
        runtime=True,
    )
    pip_audit = legal.InventoryItem(
        name="pip-audit",
        version="2.10.0",
        license="Apache-2.0",
        source="python",
        scope="python-runtime",
        runtime=True,
    )

    pylint_finding = legal.classify_item(pylint)
    pip_audit_finding = legal.classify_item(pip_audit)

    assert pylint_finding.severity == "blocked"
    assert pip_audit_finding.severity == "blocked"
    assert "dev-only" in pylint_finding.message.lower()
    assert "dev-only" in pip_audit_finding.message.lower()


def test_notice_inventory_uses_display_values_for_assets_and_long_metadata(tmp_path: Path) -> None:
    notices_path = tmp_path / "THIRD_PARTY_NOTICES.md"
    items = [
        legal.InventoryItem(
            name="pandas",
            version="3.0.2",
            license="BSD-3-Clause",
            source="python",
            scope="python-runtime",
            notice="Wheel metadata can embed full bundled dependency notices.",
        ),
        legal.InventoryItem(
            name="desktop/frontend/build/_app/immutable/assets/2.CNLA_Ghp.css",
            version=None,
            license=None,
            source="assets",
            scope="frontend-build-output",
            runtime=True,
        ),
        legal.InventoryItem(
            name="desktop/frontend/build/scriptscore-app-icon.png",
            version=None,
            license=None,
            source="assets",
            scope="frontend-asset",
            runtime=True,
        ),
    ]

    legal.write_notices(notices_path, items, [])

    notices = notices_path.read_text(encoding="utf-8")
    assert "pandas,3.0.2,BSD-3-Clause [1],python,python-runtime" in notices
    assert "## License Notes" in notices
    assert "- [1] pandas: Wheel metadata can embed full bundled dependency notices." in notices
    assert (
        "desktop/frontend/build/_app/immutable/assets/2.CNLA_Ghp.css,Not applicable,"
        "Covered by source package,assets,frontend-build-output"
    ) in notices
    assert (
        "desktop/frontend/build/scriptscore-app-icon.png,Not applicable,"
        "Release review required,assets,frontend-asset"
    ) in notices
    assert "## Review Findings" not in notices


def test_frontend_build_inventory_separates_generated_outputs_from_assets(tmp_path: Path) -> None:
    build_root = tmp_path / "desktop" / "frontend" / "build"
    files = [
        "_app/env.js",
        "_app/immutable/assets/2.CNLA_Ghp.css",
        "_app/immutable/assets/figtree-latin-wght-normal.D_ZTVpCC.woff2",
        "_app/immutable/assets/inter-latin-wght-normal.Dx4kXJAl.woff2",
        "_app/immutable/chunks/C0uQTxXd.js",
        "_app/version.json",
        "index.html",
        "scriptscore-app-icon.png",
    ]
    for file_name in files:
        path = build_root / file_name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("fixture", encoding="utf-8")

    items = legal.frontend_build_inventory(
        build_root,
        {
            "@fontsource-variable/figtree": "5.2.10",
            "@fontsource-variable/inter": "5.2.8",
        },
    )
    by_path = {Path(item.name).relative_to(build_root).as_posix(): item for item in items}
    scopes = {path: item.scope for path, item in by_path.items()}

    assert scopes["_app/env.js"] == "frontend-build-output"
    assert scopes["_app/immutable/assets/2.CNLA_Ghp.css"] == "frontend-build-output"
    assert scopes["_app/immutable/chunks/C0uQTxXd.js"] == "frontend-build-output"
    assert scopes["_app/version.json"] == "frontend-build-output"
    assert scopes["index.html"] == "frontend-build-output"
    figtree = by_path["_app/immutable/assets/figtree-latin-wght-normal.D_ZTVpCC.woff2"]
    inter = by_path["_app/immutable/assets/inter-latin-wght-normal.Dx4kXJAl.woff2"]
    assert figtree.scope == "frontend-font-asset"
    assert figtree.source == "npm"
    assert figtree.version == "5.2.10"
    assert figtree.license == "OFL-1.1"
    assert "@fontsource-variable/figtree" in figtree.notice
    assert legal.classify_item(figtree) is None
    assert inter.scope == "frontend-font-asset"
    assert inter.source == "npm"
    assert inter.version == "5.2.8"
    assert inter.license == "OFL-1.1"
    assert "@fontsource-variable/inter" in inter.notice
    assert legal.classify_item(inter) is None
    assert scopes["scriptscore-app-icon.png"] == "frontend-asset"


def test_generate_maps_fontsource_woff2_assets_without_frontend_asset_findings(
    tmp_path: Path,
) -> None:
    build_root = tmp_path / "desktop" / "frontend" / "build"
    for file_name in (
        "_app/immutable/assets/figtree-latin-ext-wght-normal.DCwSJGxG.woff2",
        "_app/immutable/assets/inter-latin-wght-normal.Dx4kXJAl.woff2",
    ):
        path = build_root / file_name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("font fixture", encoding="utf-8")

    lock_path = tmp_path / "package-lock.json"
    write_json(
        lock_path,
        {
            "packages": {
                "node_modules/@fontsource-variable/figtree": {
                    "version": "5.2.10",
                    "license": "OFL-1.1",
                    "dev": True,
                },
                "node_modules/@fontsource-variable/inter": {
                    "version": "5.2.8",
                    "license": "OFL-1.1",
                    "dev": True,
                },
            }
        },
    )
    cargo_path = tmp_path / "cargo.json"
    write_json(cargo_path, {"workspace_members": [], "packages": []})

    args = legal.parse_args(
        [
            "--check",
            "--output-dir",
            str(tmp_path / "legal"),
            "--runtime-manifest",
            str(tmp_path / "missing-runtime" / "runtime-manifest.json"),
            "--npm-lock",
            str(lock_path),
            "--cargo-metadata-file",
            str(cargo_path),
            "--frontend-build",
            str(build_root),
            "--paddle-models",
            str(tmp_path / "missing-models"),
        ]
    )

    assert legal.generate(args) == 0
    report = json.loads((tmp_path / "legal" / "license-policy-report.json").read_text())
    assert report["summary"]["findingCount"] == 0
    sbom = json.loads((tmp_path / "legal" / "sbom-assets.json").read_text())
    font_assets = {
        component["path"]: component
        for component in sbom["components"]
        if component["scope"] == "frontend-font-asset"
    }
    assert len(font_assets) == 2
    assert all(component["license"] == "OFL-1.1" for component in font_assets.values())
    assert all(component["source"] == "npm" for component in font_assets.values())
    notices = (tmp_path / "legal" / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
    assert "@fontsource-variable/figtree" in notices
    assert "@fontsource-variable/inter" in notices
    assert "frontend-asset" not in notices


def test_npm_and_cargo_inventory_parse_fixture_metadata(tmp_path: Path) -> None:
    lock_path = tmp_path / "package-lock.json"
    write_json(
        lock_path,
        {
            "packages": {
                "": {"name": "root", "version": "1.0.0"},
                "node_modules/runtime-dep": {
                    "version": "1.2.3",
                    "license": "Apache-2.0",
                },
                "node_modules/dev-dep": {
                    "version": "4.5.6",
                    "license": "MIT",
                    "dev": True,
                },
            }
        },
    )
    cargo_path = tmp_path / "cargo.json"
    write_json(
        cargo_path,
        {
            "workspace_members": ["path+file:///repo/desktop#host@0.1.0"],
            "packages": [
                {
                    "id": "path+file:///repo/desktop#host@0.1.0",
                    "name": "host",
                    "version": "0.1.0",
                    "license": "AGPL-3.0-only",
                    "manifest_path": "/repo/desktop/Cargo.toml",
                },
                {
                    "id": "registry+serde@1.0.0",
                    "name": "serde",
                    "version": "1.0.0",
                    "license": "MIT OR Apache-2.0",
                    "manifest_path": "/cargo/serde/Cargo.toml",
                },
            ],
        },
    )

    npm_items = legal.npm_inventory(lock_path)
    cargo_items = legal.cargo_inventory(tmp_path / "Cargo.toml", cargo_path)

    assert [(item.name, item.scope) for item in npm_items] == [
        ("dev-dep", "npm-dev"),
        ("runtime-dep", "npm-runtime"),
    ]
    assert [(item.name, item.scope) for item in cargo_items] == [
        ("host", "cargo-first-party"),
        ("serde", "cargo-runtime"),
    ]


def test_check_mode_fails_only_blocked_or_unknown_runtime(tmp_path: Path) -> None:
    runtime = tmp_path / "runtime"
    runtime.mkdir()
    write_json(
        runtime / "runtime-manifest.json",
        {
            "manifestVersion": 1,
            "portableRelease": True,
            "pythonExecutable": "missing-python",
        },
    )
    lock_path = tmp_path / "package-lock.json"
    write_json(lock_path, {"packages": {}})
    cargo_path = tmp_path / "cargo.json"
    write_json(cargo_path, {"workspace_members": [], "packages": []})

    args = legal.parse_args(
        [
            "--check",
            "--output-dir",
            str(tmp_path / "legal"),
            "--runtime-manifest",
            str(runtime / "runtime-manifest.json"),
            "--npm-lock",
            str(lock_path),
            "--cargo-metadata-file",
            str(cargo_path),
            "--frontend-build",
            str(tmp_path / "frontend-build"),
            "--paddle-models",
            str(tmp_path / "missing-models"),
        ]
    )

    assert legal.generate(args) == 1
    report = json.loads((tmp_path / "legal" / "license-policy-report.json").read_text())
    assert report["summary"]["blockedOrUnknownRuntimeCount"] == 1
    notices = (tmp_path / "legal" / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
    assert "## Review Findings" not in notices
    assert "missing-python" not in notices
    assert report["findings"][0]["item"] == "python-runtime"


def test_scriptscoreplus_guard_allows_placeholder_but_blocks_import(tmp_path: Path) -> None:
    root = tmp_path
    allowed = root / "desktop" / "frontend" / "src" / "lib" / "components" / "desktop"
    allowed.mkdir(parents=True)
    (allowed / "AiAssistanceStep.svelte").write_text(
        "const value = 'scriptscore_plus';\n",
        encoding="utf-8",
    )
    blocked = root / "cli" / "src"
    blocked.mkdir(parents=True)
    (blocked / "bad.py").write_text("import scriptscore_plus\n", encoding="utf-8")

    violations = guard.scan(root)

    assert len(violations) == 1
    assert "bad.py" in violations[0]

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

    assert legal.classify_item(allowed) is None
    assert legal.classify_item(unknown).severity == "unknown"
    model_finding = legal.classify_item(model)
    assert model_finding.severity == "review_required"
    assert "Asset or native binary" in model_finding.message


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

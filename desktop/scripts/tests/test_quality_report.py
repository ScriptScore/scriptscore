# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "quality_report.py"
SPEC = importlib.util.spec_from_file_location("quality_report", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload), encoding="utf-8")


class QualityReportTests(unittest.TestCase):
    def test_generate_report_renders_cli_and_desktop_sections(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            write_json(
                repo_root / "cli/coverage.json",
                {
                    "totals": {
                        "percent_covered": 91.0,
                        "percent_statements_covered": 92.0,
                        "percent_branches_covered": 80.0,
                    },
                    "files": {
                        "src/scriptscore/contracts/a.py": {
                            "summary": {"covered_lines": 95, "num_statements": 100}
                        },
                        "src/scriptscore/commands/a.py": {
                            "summary": {"covered_lines": 95, "num_statements": 100}
                        },
                        "src/scriptscore/runtime/a.py": {
                            "summary": {"covered_lines": 95, "num_statements": 100}
                        },
                        "src/scriptscore/transport/a.py": {
                            "summary": {"covered_lines": 95, "num_statements": 100}
                        },
                    },
                },
            )
            write_json(
                repo_root / "artifacts/rust-code-analysis-summary.json",
                {
                    "summary": {
                        "status": "pass",
                        "violation_count": 0,
                        "thresholds": {
                            "max_cognitive": 7.0,
                            "max_cyclomatic": 20.0,
                            "min_maintainability": 28.0,
                        },
                        "maxima": {
                            "cognitive": {
                                "name": "run_template_setup",
                                "path": str(repo_root / "desktop/src-tauri/src/state/project_lifecycle.rs"),
                                "start_line": 58,
                                "cognitive": 7.0,
                                "cyclomatic": 20.0,
                                "maintainability": 28.65,
                            },
                            "cyclomatic": {
                                "name": "await_terminal",
                                "path": str(repo_root / "desktop/src-tauri/src/worker.rs"),
                                "start_line": 186,
                                "cognitive": 4.0,
                                "cyclomatic": 20.0,
                                "maintainability": 37.16,
                            },
                        },
                        "minima": {
                            "maintainability": {
                                "name": "run_template_setup",
                                "path": str(repo_root / "desktop/src-tauri/src/state/project_lifecycle.rs"),
                                "start_line": 58,
                                "cognitive": 7.0,
                                "cyclomatic": 20.0,
                                "maintainability": 28.65,
                            }
                        },
                    },
                    "hotspots": {
                        "cognitive": [
                            {
                                "name": "run_template_setup",
                                "path": str(repo_root / "desktop/src-tauri/src/state/project_lifecycle.rs"),
                                "start_line": 58,
                                "cognitive": 7.0,
                                "cyclomatic": 20.0,
                                "maintainability": 28.65,
                            }
                        ],
                        "cyclomatic": [],
                        "maintainability": [],
                    },
                    "violations": [],
                },
            )
            (repo_root / "artifacts/coverage").mkdir(parents=True, exist_ok=True)
            (repo_root / "artifacts/coverage/cobertura.xml").write_text(
                '<coverage lines-covered="80" lines-valid="100" line-rate="0.8" branch-rate="0" />',
                encoding="utf-8",
            )
            write_json(
                repo_root / "artifacts/cargo-geiger.json",
                {
                    "packages": [
                        {
                            "package": {"id": {"name": "scriptscore-desktop-host"}},
                            "unsafety": {
                                "used": {
                                    "exprs": {"unsafe_": 0},
                                    "functions": {"unsafe_": 0},
                                    "methods": {"unsafe_": 0},
                                    "item_impls": {"unsafe_": 0},
                                    "item_traits": {"unsafe_": 0},
                                }
                            },
                        }
                    ]
                },
            )
            write_json(
                repo_root / "desktop/frontend/coverage/coverage-summary.json",
                {
                    "total": {
                        "lines": {"pct": 61.0},
                        "statements": {"pct": 63.0},
                        "branches": {"pct": 44.0},
                    }
                },
            )
            (repo_root / "sonar-project.properties").write_text("sonar.projectName=ScriptScore", encoding="utf-8")
            output_path = repo_root / "artifacts/quality-report.html"

            exit_code = MODULE.generate_report(repo_root, output_path)

            self.assertEqual(exit_code, 0)
            rendered = output_path.read_text(encoding="utf-8")
            self.assertIn("ScriptScore Quality Report", rendered)
            self.assertIn("CLI Workflow Snapshot", rendered)
            self.assertIn("Desktop Workflow Snapshot", rendered)
            self.assertIn("CLI Coverage Thresholds", rendered)
            self.assertIn("Maintainability Snapshot", rendered)
            self.assertIn("91.0%", rendered)
            self.assertIn("80.0%", rendered)

    def test_generate_report_handles_missing_optional_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            output_path = repo_root / "artifacts/quality-report.html"

            exit_code = MODULE.generate_report(repo_root, output_path)

            self.assertEqual(exit_code, 0)
            rendered = output_path.read_text(encoding="utf-8")
            self.assertIn("Missing", rendered)
            self.assertIn("No local CLI coverage artifact was found", rendered)
            self.assertIn("Rust complexity metrics are not available", rendered)

    def test_generate_report_marks_partial_cli_coverage_without_failing_gate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            write_json(
                repo_root / "cli/coverage.json",
                {
                    "totals": {
                        "percent_covered": 88.0,
                        "percent_statements_covered": 89.0,
                        "percent_branches_covered": 70.0,
                    },
                    "files": {
                        "src/scriptscore/transport/a.py": {
                            "summary": {"covered_lines": 95, "num_statements": 100}
                        }
                    },
                },
            )
            write_json(
                repo_root / "artifacts/rust-code-analysis-summary.json",
                {
                    "summary": {
                        "status": "pass",
                        "violation_count": 0,
                        "thresholds": {
                            "max_cognitive": 7.0,
                            "max_cyclomatic": 20.0,
                            "min_maintainability": 28.0,
                        },
                        "maxima": {
                            "cognitive": {
                                "name": "run_template_setup",
                                "path": str(repo_root / "desktop/src-tauri/src/state/project_lifecycle.rs"),
                                "start_line": 58,
                                "cognitive": 7.0,
                                "cyclomatic": 20.0,
                                "maintainability": 28.65,
                            },
                            "cyclomatic": {
                                "name": "await_terminal",
                                "path": str(repo_root / "desktop/src-tauri/src/worker.rs"),
                                "start_line": 186,
                                "cognitive": 4.0,
                                "cyclomatic": 20.0,
                                "maintainability": 37.16,
                            },
                        },
                        "minima": {
                            "maintainability": {
                                "name": "run_template_setup",
                                "path": str(repo_root / "desktop/src-tauri/src/state/project_lifecycle.rs"),
                                "start_line": 58,
                                "cognitive": 7.0,
                                "cyclomatic": 20.0,
                                "maintainability": 28.65,
                            }
                        },
                    },
                    "hotspots": {
                        "cognitive": [],
                        "cyclomatic": [],
                        "maintainability": [],
                    },
                    "violations": [],
                },
            )
            output_path = repo_root / "artifacts/quality-report.html"

            exit_code = MODULE.generate_report(repo_root, output_path)

            self.assertEqual(exit_code, 0)
            rendered = output_path.read_text(encoding="utf-8")
            self.assertIn("Local CLI coverage is incomplete", rendered)
            self.assertIn(">Partial<", rendered)
            self.assertNotIn("CLI coverage threshold checks failed", rendered)
            self.assertIn("All 1 measured hard gates passed.", rendered)


if __name__ == "__main__":
    unittest.main()

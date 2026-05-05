# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = (
    Path(__file__).resolve().parents[1] / "rust_code_analysis_report.py"
)
SPEC = importlib.util.spec_from_file_location("rust_code_analysis_report", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RustCodeAnalysisReportTests(unittest.TestCase):
    def test_parse_json_stream_handles_concatenated_objects(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            path = Path(tmp_dir) / "stream.json"
            path.write_text('{"name":"a"}\n{"name":"b"}\n', encoding="utf-8")

            records = MODULE.parse_json_stream(path, allow_empty=False)

        self.assertEqual([record["name"] for record in records], ["a", "b"])

    def test_report_detects_named_function_threshold_violations(self) -> None:
        src_raw = {
            "name": "desktop/src-tauri/src/example.rs",
            "start_line": 1,
            "end_line": 40,
            "kind": "unit",
            "spaces": [
                {
                    "name": "too_complex",
                    "start_line": 10,
                    "end_line": 20,
                    "kind": "function",
                    "spaces": [],
                    "metrics": {
                        "cognitive": {"sum": 16.0},
                        "cyclomatic": {"sum": 27.0},
                        "mi": {"mi_visual_studio": 26.0},
                    },
                },
                {
                    "name": "<anonymous>",
                    "start_line": 25,
                    "end_line": 25,
                    "kind": "function",
                    "spaces": [],
                    "metrics": {
                        "cognitive": {"sum": 99.0},
                        "cyclomatic": {"sum": 99.0},
                        "mi": {"mi_visual_studio": 1.0},
                    },
                },
            ],
            "metrics": {
                "cognitive": {"sum": 16.0},
                "cyclomatic": {"sum": 27.0},
                "mi": {"mi_visual_studio": 26.0},
            },
        }

        with tempfile.TemporaryDirectory() as tmp_dir:
            src_path = Path(tmp_dir) / "src.json"
            tests_path = Path(tmp_dir) / "tests.json"
            report_path = Path(tmp_dir) / "report.json"
            summary_path = Path(tmp_dir) / "summary.json"
            src_path.write_text(json.dumps(src_raw), encoding="utf-8")
            tests_path.write_text("", encoding="utf-8")

            with contextlib.redirect_stdout(io.StringIO()):
                exit_code = MODULE.generate_report(
                    src_input=src_path,
                    tests_input=tests_path,
                    report_output=report_path,
                    summary_output=summary_path,
                    tool_version="test",
                    thresholds={
                        "max_cognitive": 15.0,
                        "max_cyclomatic": 26.0,
                        "min_maintainability": 27.0,
                    },
                    hotspot_limit=5,
                )

            self.assertEqual(exit_code, 1)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["summary"]["violation_count"], 3)
            self.assertEqual(len(report["violations"]), 3)
            self.assertEqual(report["files"][0]["functions"][0]["name"], "too_complex")


if __name__ == "__main__":
    unittest.main()

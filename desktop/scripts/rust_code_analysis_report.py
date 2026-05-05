#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from json import JSONDecodeError, JSONDecoder
from pathlib import Path
from typing import Any


def metric_value(metrics: dict[str, Any], group: str, field: str) -> float | None:
    value = (metrics.get(group) or {}).get(field)
    return float(value) if isinstance(value, (int, float)) else None


def parse_json_stream(path: Path, *, allow_empty: bool) -> list[dict[str, Any]]:
    text = path.read_text(encoding="utf-8")
    if not text.strip():
        if allow_empty:
            return []
        raise ValueError(f"{path} did not contain any rust-code-analysis output")

    decoder = JSONDecoder()
    records: list[dict[str, Any]] = []
    index = 0

    while index < len(text):
        while index < len(text) and text[index].isspace():
            index += 1
        if index >= len(text):
            break
        try:
            record, next_index = decoder.raw_decode(text, index)
        except JSONDecodeError as exc:
            raise ValueError(f"failed to parse {path} near char {index}: {exc}") from exc
        if not isinstance(record, dict):
            raise ValueError(f"{path} emitted a non-object JSON record")
        records.append(record)
        index = next_index

    if not records and not allow_empty:
        raise ValueError(f"{path} did not contain any JSON records")
    return records


def collect_named_functions(node: dict[str, Any], functions: list[dict[str, Any]]) -> None:
    for child in node.get("spaces", []) or []:
        if not isinstance(child, dict):
            continue
        if child.get("kind") == "function" and child.get("name") != "<anonymous>":
            metrics = child.get("metrics") or {}
            functions.append(
                {
                    "name": child.get("name"),
                    "start_line": child.get("start_line"),
                    "end_line": child.get("end_line"),
                    "metrics": {
                        "cognitive": metric_value(metrics, "cognitive", "sum"),
                        "cyclomatic": metric_value(metrics, "cyclomatic", "sum"),
                        "maintainability": metric_value(
                            metrics, "mi", "mi_visual_studio"
                        ),
                    },
                }
            )
        collect_named_functions(child, functions)


def normalize_file_record(category: str, record: dict[str, Any]) -> dict[str, Any]:
    metrics = record.get("metrics") or {}
    functions: list[dict[str, Any]] = []
    collect_named_functions(record, functions)
    functions.sort(key=lambda item: (item["start_line"], item["name"]))

    return {
        "category": category,
        "path": record.get("name"),
        "start_line": record.get("start_line"),
        "end_line": record.get("end_line"),
        "metrics": {
            "cognitive": metric_value(metrics, "cognitive", "sum"),
            "cyclomatic": metric_value(metrics, "cyclomatic", "sum"),
            "maintainability": metric_value(metrics, "mi", "mi_visual_studio"),
        },
        "functions": functions,
    }


def flatten_functions(files: list[dict[str, Any]]) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for file_entry in files:
        for function in file_entry["functions"]:
            entries.append(
                {
                    "category": file_entry["category"],
                    "path": file_entry["path"],
                    "name": function["name"],
                    "start_line": function["start_line"],
                    "end_line": function["end_line"],
                    "cognitive": function["metrics"]["cognitive"],
                    "cyclomatic": function["metrics"]["cyclomatic"],
                    "maintainability": function["metrics"]["maintainability"],
                }
            )
    return entries


def top_entries(
    functions: list[dict[str, Any]], metric: str, *, reverse: bool, limit: int
) -> list[dict[str, Any]]:
    filtered = [item for item in functions if item[metric] is not None]
    ordered = sorted(
        filtered,
        key=lambda item: (
            item[metric],
            item["path"],
            item["start_line"],
            item["name"],
        ),
        reverse=reverse,
    )
    return ordered[:limit]


def _cognitive_allowlisted(function: dict[str, Any]) -> bool:
    """Hotspots that remain intentionally linear orchestration (worker + DB + scheduler)."""
    path = str(function.get("path") or "")
    name = str(function.get("name") or "")
    return name == "run_reserved_job" and path.endswith("state/runtime.rs")


def build_violations(
    functions: list[dict[str, Any]], thresholds: dict[str, float]
) -> list[dict[str, Any]]:
    violations: list[dict[str, Any]] = []
    for function in functions:
        if (
            function["cognitive"] is not None
            and function["cognitive"] > thresholds["max_cognitive"]
            and not _cognitive_allowlisted(function)
        ):
            violations.append(
                {
                    "metric": "cognitive",
                    "threshold": thresholds["max_cognitive"],
                    "actual": function["cognitive"],
                    "path": function["path"],
                    "function": function["name"],
                    "start_line": function["start_line"],
                }
            )
        if (
            function["cyclomatic"] is not None
            and function["cyclomatic"] > thresholds["max_cyclomatic"]
        ):
            violations.append(
                {
                    "metric": "cyclomatic",
                    "threshold": thresholds["max_cyclomatic"],
                    "actual": function["cyclomatic"],
                    "path": function["path"],
                    "function": function["name"],
                    "start_line": function["start_line"],
                }
            )
        if (
            function["maintainability"] is not None
            and function["maintainability"] < thresholds["min_maintainability"]
        ):
            violations.append(
                {
                    "metric": "maintainability",
                    "threshold": thresholds["min_maintainability"],
                    "actual": function["maintainability"],
                    "path": function["path"],
                    "function": function["name"],
                    "start_line": function["start_line"],
                }
            )
    return violations


def best_or_worst(
    functions: list[dict[str, Any]], metric: str, *, reverse: bool
) -> dict[str, Any] | None:
    candidates = [item for item in functions if item[metric] is not None]
    if not candidates:
        return None
    return sorted(
        candidates,
        key=lambda item: (item[metric], item["path"], item["start_line"], item["name"]),
        reverse=reverse,
    )[0]


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--src-input", required=True)
    parser.add_argument("--tests-input", required=True)
    parser.add_argument("--report-output", required=True)
    parser.add_argument("--summary-output", required=True)
    parser.add_argument("--tool-version", default="unknown")
    parser.add_argument(
        "--max-cognitive",
        type=float,
        default=float(os.environ.get("RUST_CODE_ANALYSIS_MAX_COGNITIVE", "7")),
    )
    parser.add_argument(
        "--max-cyclomatic",
        type=float,
        default=float(os.environ.get("RUST_CODE_ANALYSIS_MAX_CYCLOMATIC", "20")),
    )
    parser.add_argument(
        "--min-maintainability",
        type=float,
        default=float(os.environ.get("RUST_CODE_ANALYSIS_MIN_MAINTAINABILITY", "28")),
    )
    parser.add_argument(
        "--hotspot-limit",
        type=int,
        default=int(os.environ.get("RUST_CODE_ANALYSIS_HOTSPOT_LIMIT", "10")),
    )
    return parser.parse_args()


def generate_report(
    *,
    src_input: Path,
    tests_input: Path,
    report_output: Path,
    summary_output: Path,
    tool_version: str,
    thresholds: dict[str, float],
    hotspot_limit: int,
) -> int:
    src_records = parse_json_stream(src_input, allow_empty=False)
    test_records = parse_json_stream(tests_input, allow_empty=True)

    files = [
        *(normalize_file_record("src", record) for record in src_records),
        *(normalize_file_record("tests", record) for record in test_records),
    ]
    files.sort(key=lambda item: (item["category"], item["path"]))

    functions = flatten_functions(files)
    src_functions = [item for item in functions if item["category"] == "src"]
    test_functions = [item for item in functions if item["category"] == "tests"]
    violations = build_violations(src_functions, thresholds)

    hotspots = {
        "cognitive": top_entries(
            src_functions, "cognitive", reverse=True, limit=hotspot_limit
        ),
        "cyclomatic": top_entries(
            src_functions, "cyclomatic", reverse=True, limit=hotspot_limit
        ),
        "maintainability": top_entries(
            src_functions, "maintainability", reverse=False, limit=hotspot_limit
        ),
    }

    summary = {
        "status": "fail" if violations else "pass",
        "thresholds": thresholds,
        "files": {
            "src": len(src_records),
            "tests": len(test_records),
            "total": len(files),
        },
        "named_functions": {
            "src": len(src_functions),
            "tests": len(test_functions),
            "total": len(functions),
        },
        "maxima": {
            "cognitive": best_or_worst(src_functions, "cognitive", reverse=True),
            "cyclomatic": best_or_worst(src_functions, "cyclomatic", reverse=True),
        },
        "minima": {
            "maintainability": best_or_worst(
                src_functions, "maintainability", reverse=False
            ),
        },
        "violation_count": len(violations),
    }

    report = {
        "report_format_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "tool": {
            "name": "rust-code-analysis-cli",
            "version": tool_version,
        },
        "thresholds": thresholds,
        "files": files,
        "summary": summary,
        "hotspots": hotspots,
        "violations": violations,
    }
    summary_report = {
        "report_format_version": report["report_format_version"],
        "generated_at": report["generated_at"],
        "tool": report["tool"],
        "summary": summary,
        "hotspots": hotspots,
        "violations": violations,
    }

    write_json(report_output, report)
    write_json(summary_output, summary_report)

    print("Rust complexity summary")
    print(
        f"- files: src={summary['files']['src']} tests={summary['files']['tests']} total={summary['files']['total']}"
    )
    print(
        f"- named functions: src={summary['named_functions']['src']} tests={summary['named_functions']['tests']} total={summary['named_functions']['total']}"
    )
    max_cognitive = summary["maxima"]["cognitive"]
    if max_cognitive:
        print(
            f"- max cognitive: {max_cognitive['cognitive']} at {max_cognitive['path']}:{max_cognitive['start_line']} ({max_cognitive['name']})"
        )
    max_cyclomatic = summary["maxima"]["cyclomatic"]
    if max_cyclomatic:
        print(
            f"- max cyclomatic: {max_cyclomatic['cyclomatic']} at {max_cyclomatic['path']}:{max_cyclomatic['start_line']} ({max_cyclomatic['name']})"
        )
    min_maintainability = summary["minima"]["maintainability"]
    if min_maintainability:
        print(
            f"- min maintainability: {min_maintainability['maintainability']:.2f} at {min_maintainability['path']}:{min_maintainability['start_line']} ({min_maintainability['name']})"
        )
    print(
        f"- thresholds: cognitive<={thresholds['max_cognitive']} cyclomatic<={thresholds['max_cyclomatic']} maintainability>={thresholds['min_maintainability']}"
    )

    if violations:
        print("- violations:")
        for violation in violations:
            print(
                "  "
                f"{violation['metric']} {violation['actual']} at "
                f"{violation['path']}:{violation['start_line']} ({violation['function']}) "
                f"threshold={violation['threshold']}"
            )
        return 1

    print("- status: pass")
    return 0


def main() -> int:
    args = parse_args()
    thresholds = {
        "max_cognitive": args.max_cognitive,
        "max_cyclomatic": args.max_cyclomatic,
        "min_maintainability": args.min_maintainability,
    }
    return generate_report(
        src_input=Path(args.src_input),
        tests_input=Path(args.tests_input),
        report_output=Path(args.report_output),
        summary_output=Path(args.summary_output),
        tool_version=args.tool_version,
        thresholds=thresholds,
        hotspot_limit=args.hotspot_limit,
    )


if __name__ == "__main__":
    sys.exit(main())

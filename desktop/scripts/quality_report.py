#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only

from __future__ import annotations

import argparse
import html
import importlib.util
import json
import os
import textwrap
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


CLI_COVERAGE_THRESHOLDS = {
    "src/scriptscore/contracts/": 90.0,
    "src/scriptscore/commands/": 90.0,
    "src/scriptscore/runtime/": 90.0,
    "src/scriptscore/transport/": 90.0,
}

CLI_COVERAGE_EXCLUDED_PREFIXES = (
    "src/scriptscore/providers/fake.py",
    "src/scriptscore/providers/__init__.py",
)

CLI_WORKFLOW_JOBS = (
    "CLI Fast Quality",
    "CLI Test And Coverage",
    "CLI Wheel Smoke Install",
)

DESKTOP_WORKFLOW_JOBS = (
    "Desktop Frontend Quality",
    "Desktop Frontend Coverage",
    "Desktop Host Quality",
    "Desktop Rust Metrics",
    "Desktop Rust Coverage",
    "Desktop Rust Unsafe",
    "Desktop SonarCloud",
)


@dataclass
class StatusSummary:
    label: str
    tone: str
    detail: str


def load_json(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def writable_repo_path(path: Path) -> Path:
    if os.access(path, os.W_OK):
        return path
    path_str = str(path)
    if path_str.startswith("/home/"):
        candidate = Path("/var" + path_str)
        if candidate.exists() and os.access(candidate, os.W_OK):
            return candidate
    return path


def relative_path(repo_root: Path, raw_path: str | None) -> str:
    if not raw_path:
        return "n/a"
    path = Path(raw_path)
    try:
        return path.resolve().relative_to(repo_root.resolve()).as_posix()
    except ValueError:
        return raw_path


def file_coverage(entry: dict[str, Any]) -> float:
    summary = entry["summary"]
    covered = float(summary["covered_lines"])
    total = float(summary["num_statements"])
    if total == 0:
        return 100.0
    return (covered / total) * 100.0


def load_cli_thresholds(repo_root: Path) -> dict[str, float]:
    path = repo_root / "cli/tests/support/check_coverage_thresholds.py"
    if not path.exists():
        return dict(CLI_COVERAGE_THRESHOLDS)
    spec = importlib.util.spec_from_file_location("cli_coverage_thresholds", path)
    if spec is None or spec.loader is None:
        return dict(CLI_COVERAGE_THRESHOLDS)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    thresholds = getattr(module, "THRESHOLDS", CLI_COVERAGE_THRESHOLDS)
    return dict(thresholds)


def load_cli_quality(repo_root: Path) -> dict[str, Any]:
    coverage_path = repo_root / "cli/coverage.json"
    coverage = load_json(coverage_path)
    thresholds = load_cli_thresholds(repo_root)
    totals = None
    subsystem_rows: list[dict[str, Any]] = []
    failures = 0
    missing = 0

    if coverage is not None:
        totals = coverage.get("totals") or {}
        files = coverage.get("files") or {}
        for prefix, threshold in thresholds.items():
            matched = [
                file_coverage(entry)
                for path, entry in files.items()
                if path.startswith(prefix)
                and not path.startswith(CLI_COVERAGE_EXCLUDED_PREFIXES)
            ]
            if not matched:
                subsystem_rows.append(
                    {
                        "prefix": prefix,
                        "threshold": threshold,
                        "average": None,
                        "status": "missing",
                    }
                )
                missing += 1
                continue
            average = sum(matched) / len(matched)
            status = "pass" if average >= threshold else "fail"
            if status == "fail":
                failures += 1
            subsystem_rows.append(
                {
                    "prefix": prefix,
                    "threshold": threshold,
                    "average": average,
                    "status": status,
                }
            )

    if coverage is None:
        gate = StatusSummary(
            label="Missing",
            tone="missing",
            detail="No local CLI coverage artifact was found at cli/coverage.json.",
        )
    elif failures:
        gate = StatusSummary(
            label="Failed",
            tone="fail",
            detail=f"{failures} CLI coverage threshold checks failed.",
        )
    elif missing:
        gate = StatusSummary(
            label="Partial",
            tone="configured",
            detail=(
                f"Local CLI coverage is incomplete; {missing} CI-governed subsystem "
                "thresholds were not measured."
            ),
        )
    else:
        gate = StatusSummary(
            label="Passed",
            tone="pass",
            detail="All CLI subsystem coverage thresholds passed.",
        )

    return {
        "gate": gate,
        "totals": totals,
        "subsystems": subsystem_rows,
        "workflow_jobs": [
            {
                "name": "CLI Fast Quality",
                "status": StatusSummary(
                    label="Configured",
                    tone="configured",
                    detail="Ruff lint, Ruff format, and mypy are enforced in CI.",
                ),
            },
            {
                "name": "CLI Test And Coverage",
                "status": gate,
            },
            {
                "name": "CLI Wheel Smoke Install",
                "status": StatusSummary(
                    label="Configured",
                    tone="configured",
                    detail="Wheel build and install smoke checks run in CI.",
                ),
            },
        ],
    }


def load_desktop_rust_metrics(repo_root: Path) -> dict[str, Any]:
    summary = load_json(repo_root / "artifacts/rust-code-analysis-summary.json")
    if summary is None:
        return {
            "gate": StatusSummary(
                label="Missing",
                tone="missing",
                detail="No Rust complexity summary artifact was found.",
            ),
            "summary": None,
            "hotspots": {},
            "violations": [],
        }
    report_summary = summary.get("summary") or {}
    status = report_summary.get("status")
    gate = StatusSummary(
        label="Passed" if status == "pass" else "Failed",
        tone="pass" if status == "pass" else "fail",
        detail=(
            "Rust complexity thresholds passed."
            if status == "pass"
            else f"{report_summary.get('violation_count', 0)} Rust complexity violations."
        ),
    )
    return {
        "gate": gate,
        "summary": report_summary,
        "hotspots": summary.get("hotspots") or {},
        "violations": summary.get("violations") or [],
    }


def load_cobertura_coverage(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    root = ET.parse(path).getroot()
    return {
        "line_rate": float(root.attrib.get("line-rate", "0")),
        "lines_covered": int(root.attrib.get("lines-covered", "0")),
        "lines_valid": int(root.attrib.get("lines-valid", "0")),
        "branch_rate": float(root.attrib.get("branch-rate", "0")),
    }


def load_frontend_coverage(path: Path) -> dict[str, Any] | None:
    data = load_json(path)
    if data is None:
        return None
    return data.get("total")


def unsafe_package_count(geiger: dict[str, Any]) -> tuple[int, int]:
    package_count = 0
    unsafe_count = 0
    for package in geiger.get("packages", []):
        package_count += 1
        used = package.get("unsafety", {}).get("used", {})
        total = 0
        for group in used.values():
            if isinstance(group, dict):
                total += int(group.get("unsafe_", 0))
        if total > 0:
            unsafe_count += 1
    return package_count, unsafe_count


def root_package_unsafe(geiger: dict[str, Any]) -> int | None:
    for package in geiger.get("packages", []):
        package_id = package.get("package", {}).get("id", {})
        if package_id.get("name") == "scriptscore-desktop-host":
            total = 0
            used = package.get("unsafety", {}).get("used", {})
            for group in used.values():
                if isinstance(group, dict):
                    total += int(group.get("unsafe_", 0))
            return total
    return None


def load_desktop_quality(repo_root: Path) -> dict[str, Any]:
    rust_metrics = load_desktop_rust_metrics(repo_root)
    rust_coverage = load_cobertura_coverage(repo_root / "artifacts/coverage/cobertura.xml")
    frontend_coverage = load_frontend_coverage(
        repo_root / "desktop/frontend/coverage/coverage-summary.json"
    )
    geiger = load_json(repo_root / "artifacts/cargo-geiger.json")

    unsafe_packages = None
    root_unsafe = None
    if geiger is not None:
        unsafe_packages = unsafe_package_count(geiger)
        root_unsafe = root_package_unsafe(geiger)

    sonar_ready = (
        (repo_root / "sonar-project.properties").exists()
        and rust_coverage is not None
        and frontend_coverage is not None
    )

    workflow_jobs = [
        {
            "name": "Desktop Frontend Quality",
            "status": StatusSummary(
                label="Configured",
                tone="configured",
                detail="ESLint, Svelte check, tests, and build run in CI.",
            ),
        },
        {
            "name": "Desktop Frontend Coverage",
            "status": StatusSummary(
                label="Passed" if frontend_coverage else "Missing",
                tone="pass" if frontend_coverage else "missing",
                detail=(
                    "Frontend coverage artifacts are present."
                    if frontend_coverage
                    else "No local frontend coverage summary was found."
                ),
            ),
        },
        {
            "name": "Desktop Host Quality",
            "status": StatusSummary(
                label="Configured",
                tone="configured",
                detail="Rust fmt, clippy, and tests are enforced in CI.",
            ),
        },
        {
            "name": "Desktop Rust Metrics",
            "status": rust_metrics["gate"],
        },
        {
            "name": "Desktop Rust Coverage",
            "status": StatusSummary(
                label="Passed" if rust_coverage else "Missing",
                tone="pass" if rust_coverage else "missing",
                detail=(
                    "Rust coverage artifacts are present."
                    if rust_coverage
                    else "No Rust coverage artifact was found."
                ),
            ),
        },
        {
            "name": "Desktop Rust Unsafe",
            "status": StatusSummary(
                label="Informational" if geiger else "Missing",
                tone="info" if geiger else "missing",
                detail=(
                    "cargo-geiger snapshot is available."
                    if geiger
                    else "No cargo-geiger report was found."
                ),
            ),
        },
        {
            "name": "Desktop SonarCloud",
            "status": StatusSummary(
                label="Ready" if sonar_ready else "Configured",
                tone="pass" if sonar_ready else "configured",
                detail=(
                    "Local SonarCloud input artifacts are present."
                    if sonar_ready
                    else "SonarCloud is configured, but one or more local coverage inputs are missing."
                ),
            ),
        },
    ]

    return {
        "rust_metrics": rust_metrics,
        "rust_coverage": rust_coverage,
        "frontend_coverage": frontend_coverage,
        "geiger": geiger,
        "unsafe_packages": unsafe_packages,
        "root_unsafe": root_unsafe,
        "workflow_jobs": workflow_jobs,
    }


def overall_status(cli: dict[str, Any], desktop: dict[str, Any]) -> tuple[StatusSummary, int, int]:
    measured = [
        cli["gate"],
        desktop["rust_metrics"]["gate"],
    ]
    failures = sum(1 for item in measured if item.tone == "fail")
    available = sum(1 for item in measured if item.tone in {"pass", "fail"})
    if failures:
        return (
            StatusSummary(
                label="Failed",
                tone="fail",
                detail=f"{failures} measured hard gates failed.",
            ),
            failures,
            available,
        )
    if available:
        return (
            StatusSummary(
                label="Passed",
                tone="pass",
                detail=f"All {available} measured hard gates passed.",
            ),
            failures,
            available,
        )
    return (
        StatusSummary(
            label="Partial",
            tone="configured",
            detail="No measured hard-gate artifacts were available.",
        ),
        failures,
        available,
    )


def fmt_percent(value: float | None, *, digits: int = 1) -> str:
    if value is None:
        return "n/a"
    return f"{value:.{digits}f}%"


def fmt_number(value: float | int | None, *, digits: int = 2) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, int):
        return str(value)
    return f"{value:.{digits}f}"


def status_badge(status: StatusSummary) -> str:
    return (
        f'<span class="badge badge-{status.tone}">{html.escape(status.label)}</span>'
        f'<span class="card-subtitle">{html.escape(status.detail)}</span>'
    )


def overview_cards(
    overall: StatusSummary,
    failures: int,
    available: int,
    cli: dict[str, Any],
    desktop: dict[str, Any],
) -> str:
    cli_cov = None
    if cli["totals"] is not None:
        cli_cov = float(cli["totals"].get("percent_covered", 0.0))
    rust_cov = None
    if desktop["rust_coverage"] is not None:
        rust_cov = float(desktop["rust_coverage"]["line_rate"]) * 100.0
    frontend_cov = None
    if desktop["frontend_coverage"] is not None:
        frontend_cov = float(desktop["frontend_coverage"]["lines"]["pct"])

    unsafe_packages = desktop["unsafe_packages"][1] if desktop["unsafe_packages"] else None
    issue_count = (
        sum(1 for row in cli["subsystems"] if row["status"] == "fail")
        + len(desktop["rust_metrics"]["violations"])
    )

    cards = [
        (
            "Quality Gate Status",
            overall.label,
            overall.detail,
            overall.tone,
        ),
        (
            "Measured Hard Gates",
            f"{available - failures}/{available}" if available else "n/a",
            "Measured from local CLI + desktop artifacts.",
            "info",
        ),
        (
            "Measured Issues",
            str(issue_count),
            "CLI threshold failures plus desktop Rust metric violations.",
            "info" if issue_count == 0 else "fail",
        ),
        (
            "CLI Coverage",
            fmt_percent(cli_cov),
            "Overall line coverage from cli/coverage.json.",
            "pass" if cli_cov is not None else "missing",
        ),
        (
            "Rust Coverage",
            fmt_percent(rust_cov),
            "Desktop Rust line coverage from Tarpaulin Cobertura output.",
            "pass" if rust_cov is not None else "missing",
        ),
        (
            "Frontend Coverage",
            fmt_percent(frontend_cov),
            "Desktop frontend line coverage from coverage-summary.json.",
            "pass" if frontend_cov is not None else "missing",
        ),
        (
            "Unsafe Packages",
            fmt_number(unsafe_packages, digits=0),
            "Packages in the desktop Rust dependency graph with used unsafe code.",
            "info" if unsafe_packages is not None else "missing",
        ),
    ]
    return "\n".join(
        f"""
        <article class="metric-card">
          <div class="card-label">{html.escape(label)}</div>
          <div class="card-value tone-{tone}">{html.escape(value)}</div>
          <div class="card-subtitle">{html.escape(detail)}</div>
        </article>
        """.strip()
        for label, value, detail, tone in cards
    )


def render_job_table(title: str, jobs: list[dict[str, Any]]) -> str:
    rows = "\n".join(
        f"""
        <tr>
          <td>{html.escape(job['name'])}</td>
          <td><span class="badge badge-{job['status'].tone}">{html.escape(job['status'].label)}</span></td>
          <td>{html.escape(job['status'].detail)}</td>
        </tr>
        """.strip()
        for job in jobs
    )
    return f"""
    <section class="panel">
      <div class="panel-header">
        <h2>{html.escape(title)}</h2>
      </div>
      <table class="status-table">
        <thead>
          <tr><th>Job</th><th>Status</th><th>Detail</th></tr>
        </thead>
        <tbody>
          {rows}
        </tbody>
      </table>
    </section>
    """


def render_cli_subsystems(cli: dict[str, Any]) -> str:
    rows = cli["subsystems"]
    if not rows:
        body = '<div class="empty-state">CLI coverage thresholds were not measured locally.</div>'
    else:
        body = """
        <table class="status-table">
          <thead>
            <tr><th>Subsystem</th><th>Threshold</th><th>Average</th><th>Status</th></tr>
          </thead>
          <tbody>
        """
        for row in rows:
            average = "n/a" if row["average"] is None else fmt_percent(row["average"], digits=2)
            body += f"""
            <tr>
              <td>{html.escape(row['prefix'])}</td>
              <td>{fmt_percent(float(row['threshold']), digits=0)}</td>
              <td>{average}</td>
              <td><span class="badge badge-{row['status']}">{html.escape(row['status'].title())}</span></td>
            </tr>
            """
        body += "</tbody></table>"
    return f"""
    <section class="panel">
      <div class="panel-header">
        <h2>CLI Coverage Thresholds</h2>
        {status_badge(cli['gate'])}
      </div>
      {body}
    </section>
    """


def render_coverage_panel(cli: dict[str, Any], desktop: dict[str, Any]) -> str:
    cli_totals = cli["totals"] or {}
    rust = desktop["rust_coverage"]
    frontend = desktop["frontend_coverage"]
    return f"""
    <section class="panel">
      <div class="panel-header">
        <h2>Coverage Snapshot</h2>
      </div>
      <div class="split-grid">
        <div class="mini-card">
          <div class="card-label">CLI</div>
          <div class="metric-line">Lines: {fmt_percent(cli_totals.get('percent_covered') if cli_totals else None)}</div>
          <div class="metric-line">Statements: {fmt_percent(cli_totals.get('percent_statements_covered') if cli_totals else None)}</div>
          <div class="metric-line">Branches: {fmt_percent(cli_totals.get('percent_branches_covered') if cli_totals else None)}</div>
        </div>
        <div class="mini-card">
          <div class="card-label">Desktop Rust</div>
          <div class="metric-line">Lines: {fmt_percent(float(rust['line_rate']) * 100.0 if rust else None)}</div>
          <div class="metric-line">Covered lines: {fmt_number(rust['lines_covered'], digits=0) if rust else 'n/a'}</div>
          <div class="metric-line">Total lines: {fmt_number(rust['lines_valid'], digits=0) if rust else 'n/a'}</div>
        </div>
        <div class="mini-card">
          <div class="card-label">Desktop Frontend</div>
          <div class="metric-line">Lines: {fmt_percent(frontend['lines']['pct'] if frontend else None)}</div>
          <div class="metric-line">Statements: {fmt_percent(frontend['statements']['pct'] if frontend else None)}</div>
          <div class="metric-line">Branches: {fmt_percent(frontend['branches']['pct'] if frontend else None)}</div>
        </div>
      </div>
    </section>
    """


def render_maintainability_panel(repo_root: Path, desktop: dict[str, Any]) -> str:
    summary = desktop["rust_metrics"]["summary"]
    if summary is None:
        return """
        <section class="panel">
          <div class="panel-header">
            <h2>Maintainability Snapshot</h2>
          </div>
          <div class="empty-state">Rust complexity metrics are not available.</div>
        </section>
        """

    maxima = summary["maxima"]
    minima = summary["minima"]
    thresholds = summary["thresholds"]
    rows = []
    for metric, entries in desktop["rust_metrics"]["hotspots"].items():
        for entry in entries[:5]:
            rows.append(
                f"""
                <tr>
                  <td>{html.escape(metric.title())}</td>
                  <td>{html.escape(entry['name'])}</td>
                  <td>{html.escape(relative_path(repo_root, entry['path']))}:{entry['start_line']}</td>
                  <td>{fmt_number(entry.get(metric.lower() if metric != 'maintainability' else 'maintainability'))}</td>
                </tr>
                """
            )
    hotspots = (
        """
        <table class="status-table">
          <thead>
            <tr><th>Metric</th><th>Function</th><th>Location</th><th>Value</th></tr>
          </thead>
          <tbody>
        """
        + "".join(rows)
        + "</tbody></table>"
        if rows
        else '<div class="empty-state">No desktop Rust hotspots were recorded.</div>'
    )
    return f"""
    <section class="panel">
      <div class="panel-header">
        <h2>Maintainability Snapshot</h2>
        {status_badge(desktop['rust_metrics']['gate'])}
      </div>
      <div class="split-grid">
        <div class="mini-card">
          <div class="card-label">Max Cognitive</div>
          <div class="metric-line">{fmt_number(maxima['cognitive']['cognitive'])}</div>
          <div class="metric-line">{html.escape(maxima['cognitive']['name'])}</div>
        </div>
        <div class="mini-card">
          <div class="card-label">Max Cyclomatic</div>
          <div class="metric-line">{fmt_number(maxima['cyclomatic']['cyclomatic'])}</div>
          <div class="metric-line">{html.escape(maxima['cyclomatic']['name'])}</div>
        </div>
        <div class="mini-card">
          <div class="card-label">Min Maintainability</div>
          <div class="metric-line">{fmt_number(minima['maintainability']['maintainability'])}</div>
          <div class="metric-line">{html.escape(minima['maintainability']['name'])}</div>
        </div>
        <div class="mini-card">
          <div class="card-label">Thresholds</div>
          <div class="metric-line">Cognitive ≤ {fmt_number(thresholds['max_cognitive'])}</div>
          <div class="metric-line">Cyclomatic ≤ {fmt_number(thresholds['max_cyclomatic'])}</div>
          <div class="metric-line">Maintainability ≥ {fmt_number(thresholds['min_maintainability'])}</div>
        </div>
      </div>
      {hotspots}
    </section>
    """


def render_unsafe_panel(desktop: dict[str, Any]) -> str:
    geiger = desktop["geiger"]
    if geiger is None:
        body = '<div class="empty-state">cargo-geiger output is not available.</div>'
    else:
        scanned, unsafe_count = desktop["unsafe_packages"]
        root_unsafe = desktop["root_unsafe"]
        root_line = "n/a" if root_unsafe is None else str(root_unsafe)
        body = f"""
        <div class="split-grid">
          <div class="mini-card">
            <div class="card-label">Scanned Packages</div>
            <div class="metric-line">{scanned}</div>
          </div>
          <div class="mini-card">
            <div class="card-label">Packages With Used Unsafe</div>
            <div class="metric-line">{unsafe_count}</div>
          </div>
          <div class="mini-card">
            <div class="card-label">Root Crate Unsafe Uses</div>
            <div class="metric-line">{root_line}</div>
          </div>
        </div>
        """
    return f"""
    <section class="panel">
      <div class="panel-header">
        <h2>Unsafe Rust Snapshot</h2>
        <span class="badge badge-info">Informational</span>
      </div>
      {body}
    </section>
    """


def render_notes() -> str:
    return """
    <section class="panel">
      <div class="panel-header">
        <h2>Notes</h2>
      </div>
      <ul class="notes-list">
        <li>This is a static Sonar-style summary generated from local and CI artifacts, not from live SonarCloud APIs.</li>
        <li>Jobs marked <strong>Configured</strong> are enforced in GitHub Actions but do not currently emit a local machine-readable artifact.</li>
        <li>Measured hard gates in this report come from CLI coverage thresholds and desktop Rust complexity thresholds.</li>
      </ul>
    </section>
    """


def build_html(repo_root: Path) -> str:
    cli = load_cli_quality(repo_root)
    desktop = load_desktop_quality(repo_root)
    overall, failures, available = overall_status(cli, desktop)
    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    return f"""<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>ScriptScore Quality Report</title>
    <style>
      :root {{
        --bg: #121825;
        --panel: #1c2436;
        --panel-2: #222c41;
        --border: #33405c;
        --text: #edf2ff;
        --muted: #9da9c6;
        --pass: #2eb67d;
        --fail: #ff7d7d;
        --info: #6ac4ff;
        --configured: #bba3ff;
        --missing: #7f8ba8;
        --shadow: rgba(0, 0, 0, 0.22);
      }}
      * {{ box-sizing: border-box; }}
      body {{
        margin: 0;
        font-family: "Segoe UI", "Helvetica Neue", Helvetica, Arial, sans-serif;
        background:
          radial-gradient(circle at top left, rgba(122, 167, 255, 0.16), transparent 28%),
          linear-gradient(180deg, #0f1522 0%, var(--bg) 100%);
        color: var(--text);
      }}
      .page {{
        max-width: 1320px;
        margin: 0 auto;
        padding: 28px 24px 48px;
      }}
      .hero {{
        display: flex;
        justify-content: space-between;
        gap: 16px;
        align-items: flex-end;
        margin-bottom: 20px;
      }}
      h1 {{
        margin: 0;
        font-size: 2rem;
        letter-spacing: 0.01em;
      }}
      .subtitle {{
        margin-top: 8px;
        color: var(--muted);
      }}
      .cards {{
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: 14px;
        margin-bottom: 20px;
      }}
      .metric-card, .panel, .mini-card {{
        background: linear-gradient(180deg, rgba(255,255,255,0.02), rgba(255,255,255,0.00)), var(--panel);
        border: 1px solid var(--border);
        border-radius: 14px;
        box-shadow: 0 12px 28px var(--shadow);
      }}
      .metric-card {{
        padding: 16px;
        min-height: 116px;
      }}
      .panel {{
        padding: 18px;
        margin-bottom: 18px;
      }}
      .panel-header {{
        display: flex;
        gap: 12px;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 14px;
      }}
      .panel-header h2 {{
        margin: 0;
        font-size: 1rem;
        font-weight: 600;
      }}
      .card-label {{
        color: var(--muted);
        font-size: 0.78rem;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }}
      .card-value {{
        margin-top: 10px;
        font-size: 2rem;
        font-weight: 700;
      }}
      .card-subtitle {{
        display: block;
        margin-top: 10px;
        color: var(--muted);
        line-height: 1.45;
      }}
      .tone-pass {{ color: var(--pass); }}
      .tone-fail {{ color: var(--fail); }}
      .tone-info {{ color: var(--info); }}
      .tone-missing {{ color: var(--missing); }}
      .badge {{
        display: inline-flex;
        align-items: center;
        padding: 4px 10px;
        border-radius: 999px;
        font-size: 0.78rem;
        font-weight: 600;
        border: 1px solid transparent;
      }}
      .badge-pass {{ color: #c9ffe6; background: rgba(46, 182, 125, 0.16); border-color: rgba(46, 182, 125, 0.28); }}
      .badge-fail {{ color: #ffd9d9; background: rgba(255, 125, 125, 0.16); border-color: rgba(255, 125, 125, 0.30); }}
      .badge-info {{ color: #d8f0ff; background: rgba(106, 196, 255, 0.14); border-color: rgba(106, 196, 255, 0.28); }}
      .badge-configured {{ color: #e8dfff; background: rgba(187, 163, 255, 0.14); border-color: rgba(187, 163, 255, 0.30); }}
      .badge-missing {{ color: #d7ddea; background: rgba(127, 139, 168, 0.14); border-color: rgba(127, 139, 168, 0.30); }}
      .status-table {{
        width: 100%;
        border-collapse: collapse;
      }}
      .status-table th, .status-table td {{
        padding: 11px 10px;
        border-top: 1px solid rgba(255,255,255,0.06);
        vertical-align: top;
        text-align: left;
      }}
      .status-table th {{
        color: var(--muted);
        font-weight: 600;
        font-size: 0.84rem;
      }}
      .split-grid {{
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
        gap: 14px;
      }}
      .mini-card {{
        padding: 14px;
      }}
      .metric-line {{
        margin-top: 8px;
        font-size: 1.02rem;
      }}
      .empty-state {{
        padding: 18px;
        border-radius: 12px;
        background: var(--panel-2);
        color: var(--muted);
      }}
      .notes-list {{
        margin: 0;
        padding-left: 18px;
        color: var(--muted);
      }}
      .section-grid {{
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 18px;
      }}
      @media (max-width: 980px) {{
        .section-grid {{ grid-template-columns: 1fr; }}
        .hero {{ flex-direction: column; align-items: flex-start; }}
      }}
    </style>
  </head>
  <body>
    <div class="page">
      <header class="hero">
        <div>
          <h1>ScriptScore Quality Report</h1>
          <div class="subtitle">Generated {html.escape(generated_at)} from local quality artifacts and current CI rules.</div>
        </div>
        <div>{status_badge(overall)}</div>
      </header>

      <section class="cards">
        {overview_cards(overall, failures, available, cli, desktop)}
      </section>

      <div class="section-grid">
        {render_job_table("CLI Workflow Snapshot", cli["workflow_jobs"])}
        {render_job_table("Desktop Workflow Snapshot", desktop["workflow_jobs"])}
      </div>

      <div class="section-grid">
        {render_cli_subsystems(cli)}
        {render_maintainability_panel(repo_root, desktop)}
      </div>

      {render_coverage_panel(cli, desktop)}
      {render_unsafe_panel(desktop)}
      {render_notes()}
    </div>
  </body>
</html>
"""


def generate_report(repo_root: Path, output_path: Path) -> int:
    report = build_html(repo_root)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(report, encoding="utf-8")
    print(f"Wrote {output_path}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--repo-root",
        default=Path.cwd(),
        type=Path,
    )
    parser.add_argument(
        "--output",
        default=None,
        type=Path,
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = writable_repo_path(args.repo_root.resolve())
    output_path = args.output or (repo_root / "artifacts/quality-report.html")
    return generate_report(repo_root, output_path)


if __name__ == "__main__":
    raise SystemExit(main())

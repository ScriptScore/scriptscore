#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Fetch open SonarCloud issues into agent-readable files under artifacts/.

SonarScanner does not write issue lists to disk; this uses the Web API after analysis
finishes on SonarCloud (often 30–120s after `sonar-scanner` exits).

Environment (same as `make sonar-local`):
  SONAR_TOKEN            required
  SONAR_ORGANIZATION     required
  SONAR_PROJECT_KEY      required (same as sonar.projectKey)
  SONAR_HOST_URL         optional, default https://sonarcloud.io

Writes:
  artifacts/sonar-cloud-issues.json   full API payload (paginated merge)
  artifacts/sonar-cloud-issues.md       compact list for humans/agents
"""

from __future__ import annotations

import base64
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


def _auth_header(token: str) -> str:
    raw = f"{token}:".encode("utf-8")
    return "Basic " + base64.b64encode(raw).decode("ascii")


def _fetch_page(
    base: str,
    token: str,
    organization: str,
    component_key: str,
    page: int,
    page_size: int,
) -> dict:
    q = urllib.parse.urlencode(
        {
            "organization": organization,
            "componentKeys": component_key,
            "resolved": "false",
            "ps": str(page_size),
            "p": str(page),
        }
    )
    url = f"{base.rstrip('/')}/api/issues/search?{q}"
    req = urllib.request.Request(
        url,
        headers={"Authorization": _auth_header(token)},
        method="GET",
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.loads(resp.read().decode("utf-8"))


def fetch_all_issues(
    base: str,
    token: str,
    organization: str,
    component_key: str,
    page_size: int = 100,
) -> list[dict]:
    all_issues: list[dict] = []
    page = 1
    while True:
        data = _fetch_page(base, token, organization, component_key, page, page_size)
        issues = data.get("issues") or []
        all_issues.extend(issues)
        paging = data.get("paging") or {}
        total = int(paging.get("total", len(all_issues)))
        if not issues or len(all_issues) >= total:
            break
        page += 1
    return all_issues


def write_markdown(path: Path, issues: list[dict], component_key: str) -> None:
    lines = [
        "# SonarCloud open issues",
        "",
        f"Project: `{component_key}`",
        f"Count: {len(issues)}",
        "",
    ]
    for i in issues:
        severity = i.get("severity", "?")
        rule = i.get("rule", "?")
        msg = (i.get("message") or "").replace("\n", " ")
        comp = i.get("component", "")
        # component is often "org_project:path/to/file"
        if ":" in comp:
            comp = comp.split(":", 1)[1]
        line = i.get("line")
        loc = f"{comp}:{line}" if line else comp
        lines.append(f"- **{severity}** `{loc}` — {msg}")
        lines.append(f"  - rule: `{rule}`")
        lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    token = os.environ.get("SONAR_TOKEN", "").strip()
    org = os.environ.get("SONAR_ORGANIZATION", "").strip()
    project_key = os.environ.get("SONAR_PROJECT_KEY", "").strip()
    base = os.environ.get("SONAR_HOST_URL", "https://sonarcloud.io").strip()

    if not token or not org or not project_key:
        print(
            "Missing env: need SONAR_TOKEN, SONAR_ORGANIZATION, SONAR_PROJECT_KEY",
            file=sys.stderr,
        )
        return 1

    repo_root = Path(__file__).resolve().parents[2]
    artifacts = repo_root / "artifacts"
    artifacts.mkdir(parents=True, exist_ok=True)

    optional_wait = int(os.environ.get("SONAR_EXPORT_WAIT_SEC", "0"))
    if optional_wait > 0:
        print(f"Waiting {optional_wait}s for SonarCloud analysis…", flush=True)
        time.sleep(optional_wait)

    try:
        issues = fetch_all_issues(base, token, org, project_key)
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace") if e.fp else ""
        print(f"SonarCloud API HTTP {e.code}: {body[:500]}", file=sys.stderr)
        return 1
    except urllib.error.URLError as e:
        print(f"SonarCloud API error: {e}", file=sys.stderr)
        return 1

    out_json = artifacts / "sonar-cloud-issues.json"
    out_md = artifacts / "sonar-cloud-issues.md"
    payload = {
        "componentKey": project_key,
        "organization": org,
        "host": base,
        "issueCount": len(issues),
        "issues": issues,
    }
    out_json.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    write_markdown(out_md, issues, project_key)

    print(f"Wrote {out_json}")
    print(f"Wrote {out_md}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

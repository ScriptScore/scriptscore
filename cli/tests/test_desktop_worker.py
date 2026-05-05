# SPDX-License-Identifier: AGPL-3.0-only
"""Desktop worker subprocess smoke tests."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

from tests.support.pdfs import TemplateQuestionSpec, make_template_pdf
from tests.support.process import DesktopWorkerSession

pytestmark = pytest.mark.skipif(sys.platform == "win32", reason="Uses Unix sockets on test hosts.")


def test_desktop_worker_hello_and_smoke_ping() -> None:
    try:
        session = DesktopWorkerSession()
    except RuntimeError as exc:
        pytest.skip(str(exc))
    with session:
        session.send(
            {
                "type": "hello",
                "request_id": "req_hello",
                "payload": {"protocol_version": "desktop.sidecar.v1"},
            }
        )
        hello = session.next_message()
        assert hello["type"] == "hello_ok"
        assert hello["payload"]["protocol_version"] == "desktop.sidecar.v1"

        session.send(
            {
                "type": "run_job",
                "request_id": "req_run",
                "job_id": "job_1",
                "payload": {
                    "command_name": "smoke.ping",
                    "request": {"message": "hello", "steps": 1},
                },
            }
        )
        while True:
            message = session.next_message(timeout=5)
            if message["type"] == "job_finished":
                assert message["job_id"] == "job_1"
                assert message["payload"]["envelope"]["data"]["message"] == "hello"
                break


def test_desktop_worker_cancel_maps_to_cancelled_terminal_event() -> None:
    try:
        session = DesktopWorkerSession()
    except RuntimeError as exc:
        pytest.skip(str(exc))
    with session:
        session.send(
            {
                "type": "hello",
                "request_id": "req_hello",
                "payload": {"protocol_version": "desktop.sidecar.v1"},
            }
        )
        assert session.next_message()["type"] == "hello_ok"

        session.send(
            {
                "type": "run_job",
                "request_id": "req_run",
                "job_id": "job_1",
                "payload": {
                    "command_name": "smoke.ping",
                    "request": {"message": "slow", "steps": 10, "sleep_ms": 1000},
                },
            }
        )
        while True:
            message = session.next_message(timeout=5)
            if message["type"] == "job_progress":
                session.send(
                    {
                        "type": "cancel_job",
                        "request_id": "req_cancel",
                        "job_id": "job_1",
                        "payload": {},
                    }
                )
            if message["type"] == "job_cancelled":
                assert message["payload"]["envelope"]["error"]["category"] == "cancelled"
                break


def test_desktop_worker_runs_exam_setup(tmp_path: Path) -> None:
    try:
        session = DesktopWorkerSession()
    except RuntimeError as exc:
        pytest.skip(str(exc))

    template_pdf = make_template_pdf(
        tmp_path / "template.pdf",
        questions=[
            TemplateQuestionSpec(number=1, text="Explain westward expansion.", points=5, y=120),
            TemplateQuestionSpec(number=2, text="Describe Jacksonian democracy.", points=4, y=260),
        ],
    )
    output_dir = (tmp_path / "setup_out").resolve()

    with session:
        session.send(
            {
                "type": "hello",
                "request_id": "req_hello",
                "payload": {"protocol_version": "desktop.sidecar.v1"},
            }
        )
        assert session.next_message()["type"] == "hello_ok"

        session.send(
            {
                "type": "run_job",
                "request_id": "req_setup",
                "job_id": "job_setup",
                "payload": {
                    "command_name": "exam.setup",
                    "request": {"template_pdf_path": str(template_pdf)},
                    "output_artifacts_dir": str(output_dir),
                },
            }
        )

        while True:
            message = session.next_message(timeout=5)
            if message["type"] == "job_finished":
                envelope = message["payload"]["envelope"]
                assert envelope["command"] == "exam.setup"
                assert len(envelope["data"]["template_pages"]) == 1
                assert len(envelope["data"]["questions"]) == 2
                break

# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `grading export`."""

from __future__ import annotations

import base64
from html import escape
from pathlib import Path

from scriptscore.commands.common import file_artifact, progress
from scriptscore.contracts import (
    ErrorCategory,
    ExportQuestion,
    ExportRequest,
    ExportResult,
    GradingExportRequest,
    ScriptscoreError,
    WriteState,
)
from scriptscore.paths import join_under_root
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


def _html_escape_multiline(text: str) -> str:
    return escape(text).replace("\n", "<br>\n")


def _render_highlighted_answer(question: ExportQuestion) -> str:
    if not question.highlights:
        return _html_escape_multiline(question.student_answer)

    parts: list[str] = []
    cursor = 0
    for highlight in question.highlights:
        if cursor < highlight.start_char:
            parts.append(
                _html_escape_multiline(question.student_answer[cursor : highlight.start_char])
            )
        snippet = question.student_answer[highlight.start_char : highlight.end_char]
        parts.append(
            f'<span class="highlight highlight-{highlight.kind}">{_html_escape_multiline(snippet)}</span>'
        )
        cursor = highlight.end_char
    if cursor < len(question.student_answer):
        parts.append(_html_escape_multiline(question.student_answer[cursor:]))
    return "".join(parts)


def _inline_png_data_uri(path: Path) -> str:
    encoded = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:image/png;base64,{encoded}"


def _render_export_html(request_row: ExportRequest) -> str:
    question_cards: list[str] = []
    for question in request_row.questions:
        feedback_html = ""
        if question.feedback_text:
            feedback_html = f'<section class="feedback-block"><h4>Feedback</h4><p>{_html_escape_multiline(question.feedback_text)}</p></section>'
        question_cards.append(
            """
            <article class="question-card">
              <header class="question-header">
                <h2>{question_id}</h2>
                <div class="score-chip">{awarded} / {max_points}</div>
              </header>
              <section class="question-block">
                <h3>Question</h3>
                <p>{question_text}</p>
              </section>
              <section class="answer-layout">
                <div class="answer-block">
                  <h3>Parsed Answer</h3>
                  <div class="answer-text">{answer_html}</div>
                  {feedback_html}
                </div>
                <div class="image-block">
                  <h3>Cropped Answer Image</h3>
                  <img alt="{question_id} answer image" src="{image_src}" />
                </div>
              </section>
            </article>
            """.format(
                question_id=escape(question.question_id),
                awarded=question.total_points_awarded,
                max_points=question.question_max_points,
                question_text=_html_escape_multiline(question.question_text_clean),
                answer_html=_render_highlighted_answer(question),
                feedback_html=feedback_html,
                image_src=_inline_png_data_uri(question.question_crop_path),
            )
        )

    display_name = request_row.student_display_name or request_row.student_ref
    return """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>ScriptScore Export - {student_ref}</title>
  <style>
    :root {{
      color-scheme: light;
      --ink: #1f2937;
      --muted: #6b7280;
      --line: #d1d5db;
      --panel: #ffffff;
      --bg: #f3f4f6;
      --accent: #0f766e;
      --good: #166534;
      --bad: #991b1b;
      --neutral: #475569;
    }}
    body {{
      margin: 0;
      font-family: Georgia, "Times New Roman", serif;
      color: var(--ink);
      background: linear-gradient(180deg, #f8fafc 0%, var(--bg) 100%);
    }}
    main {{
      max-width: 1080px;
      margin: 0 auto;
      padding: 32px 24px 48px;
    }}
    .student-header {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 24px;
      margin-bottom: 24px;
      box-shadow: 0 12px 32px rgba(15, 23, 42, 0.06);
    }}
    .student-header p {{
      margin: 4px 0 0;
      color: var(--muted);
    }}
    .question-card {{
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 24px;
      margin-bottom: 20px;
      box-shadow: 0 12px 32px rgba(15, 23, 42, 0.05);
    }}
    .question-header {{
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 16px;
      margin-bottom: 18px;
    }}
    .question-header h2,
    .question-block h3,
    .answer-block h3,
    .image-block h3,
    .feedback-block h4 {{
      margin: 0;
    }}
    .score-chip {{
      padding: 6px 12px;
      border-radius: 999px;
      background: #ecfeff;
      color: var(--accent);
      font-weight: 700;
    }}
    .answer-layout {{
      display: grid;
      grid-template-columns: minmax(0, 1.4fr) minmax(320px, 1fr);
      gap: 20px;
    }}
    .answer-text {{
      white-space: pre-wrap;
      line-height: 1.6;
      border: 1px solid var(--line);
      border-radius: 12px;
      padding: 16px;
      background: #f8fafc;
    }}
    .highlight {{
      border-radius: 4px;
      padding: 0 2px;
    }}
    .highlight-correct {{
      background: #dcfce7;
      color: var(--good);
    }}
    .highlight-incorrect {{
      background: #fee2e2;
      color: var(--bad);
    }}
    .highlight-neutral {{
      background: #e2e8f0;
      color: var(--neutral);
    }}
    .feedback-block {{
      margin-top: 16px;
    }}
    .feedback-block p {{
      margin: 8px 0 0;
    }}
    .image-block img {{
      display: block;
      width: 100%;
      border-radius: 12px;
      border: 1px solid var(--line);
      background: #fff;
    }}
    @media (max-width: 900px) {{
      .answer-layout {{
        grid-template-columns: 1fr;
      }}
    }}
  </style>
</head>
<body>
  <main>
    <section class="student-header">
      <h1>{display_name}</h1>
      <p>{student_ref}</p>
    </section>
    {question_cards}
  </main>
</body>
</html>
""".format(
        student_ref=escape(request_row.student_ref),
        display_name=escape(display_name),
        question_cards="\n".join(question_cards),
    )


def handle_grading_export(ctx: CommandContext, request: GradingExportRequest) -> CommandOutcome:
    """Assemble one self-contained HTML grading export per student row."""

    for request_row in request.export_requests:
        for question in request_row.questions:
            if not question.question_crop_path.exists():
                raise ScriptscoreError(
                    code="input_artifact_not_found",
                    message=f"Referenced crop image '{question.question_crop_path}' was not found.",
                    category=ErrorCategory.NOT_FOUND,
                    retryable=False,
                    details={"path": str(question.question_crop_path)},
                    write_state=WriteState.NO_WRITE,
                )

    total = len(request.export_requests)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 1},
    )

    results: list[ExportResult] = []
    artifacts = []
    for index, request_row in enumerate(request.export_requests, start=1):
        ctx.check_cancelled()
        scope: dict[str, object] = {"student_ref": request_row.student_ref}
        ctx.emit(
            event="item_started",
            progress=progress(completed=index - 1, total=total),
            scope=scope,
        )
        html_path = join_under_root(
            request.output_artifacts_dir, f"{request_row.student_ref}_result.html"
        )
        html_path.parent.mkdir(parents=True, exist_ok=True)
        html_path.write_text(_render_export_html(request_row), encoding="utf-8")
        result = ExportResult(student_ref=request_row.student_ref, html_path=html_path, warnings=[])
        results.append(result)
        artifacts.append(
            file_artifact(
                role="student_export_html",
                label=html_path.name,
                path=html_path,
                fmt="html",
                scope=scope,
            )
        )
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"html_path": str(html_path)},
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={"target_count": total},
    )
    return CommandOutcome(
        data={
            "exports": [result.model_dump(mode="json", exclude_none=True) for result in results],
            "output_metadata_path": str(
                (request.output_artifacts_dir / "output_metadata.json").resolve()
            ),
        },
        artifacts=artifacts,
        output_artifacts_dir=request.output_artifacts_dir,
        manifest_data={
            "result_row_count": len(results),
            "written_artifact_count": len(artifacts),
            "failed_count": 0,
        },
    )


def grading_export_spec() -> CommandSpec:
    return CommandSpec(
        name="grading.export", request_model=GradingExportRequest, handler=handle_grading_export
    )

# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for Phase 3 exam commands."""

from __future__ import annotations

import json
from collections.abc import Callable
from pathlib import Path

import fitz  # type: ignore[import-untyped]
from PIL import Image

from scriptscore.artifacts.pdfs import PDF_RENDER_SCALE, open_pdf_document
from scriptscore.commands import build_command_registry
from scriptscore.commands.exam_shared import collapse_markers, detect_question_markers
from scriptscore.contracts import (
    CommandErrorEnvelope,
    CommandSuccessEnvelope,
    ErrorCategory,
    LlmTokenUsage,
    ScriptscoreError,
    WriteState,
)
from scriptscore.providers import (
    FakeLlmProvider,
    LlmProviderConfig,
    LlmRequest,
    LlmResponse,
    ProviderRegistry,
)
from scriptscore.runtime import CommandRunner
from tests.support.llm import llm_request_fields
from tests.support.pdfs import (
    TemplateQuestionSpec,
    make_template_pdf,
    make_template_pdf_with_cover_point_distribution,
    make_template_pdf_with_cover_score_table,
    make_unstructured_pdf,
)


def _runner(*, provider_registry: ProviderRegistry | None = None) -> CommandRunner:
    return CommandRunner(
        registry=build_command_registry(),
        provider_registry=provider_registry or ProviderRegistry.with_builtin_fakes(),
    )


def _registry_with_llm(
    responder: Callable[[LlmRequest], LlmResponse],
    *,
    provider_name: str = "ollama_native",
) -> ProviderRegistry:
    registry = ProviderRegistry.with_builtin_fakes()
    registry.register(FakeLlmProvider(provider_name=provider_name, responder=responder))
    return registry


def test_exam_setup_renders_template_pages_and_question_crops(tmp_path: Path) -> None:
    template_pdf = make_template_pdf(
        tmp_path / "template.pdf",
        questions=[
            TemplateQuestionSpec(number=1, text="What is 2 + 2?", points=5, y=120),
            TemplateQuestionSpec(number=2, text="Name a sorting algorithm.", points=3, y=260),
        ],
    )
    output_dir = (tmp_path / "setup_out").resolve()

    result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(template_pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    questions = result.envelope.data["questions"]
    assert [question["question_id"] for question in questions] == ["q1", "q2"]
    assert [question["max_points"] for question in questions] == [5, 3]
    assert "What is 2 + 2?" in questions[0]["baseline_pdf_text"]
    template_pages = result.envelope.data["template_pages"]
    assert len(template_pages) == 1
    assert Path(template_pages[0]["image_path"]).exists()
    with Image.open(template_pages[0]["image_path"]) as rendered_page:
        assert rendered_page.width == 900
        assert questions[0]["region"]["x"] + questions[0]["region"]["width"] <= rendered_page.width
        assert (
            questions[0]["region"]["y"] + questions[0]["region"]["height"] <= rendered_page.height
        )
    assert Path(questions[0]["template_question_png_path"]).exists()
    assert Path(questions[1]["template_question_png_path"]).exists()
    manifest = json.loads((output_dir / "output_metadata.json").read_text(encoding="utf-8"))
    assert manifest["data"] == {
        "failed_count": 0,
        "result_row_count": 2,
        "written_artifact_count": 3,
    }


def test_exam_setup_unsupported_layout_hard_fails_without_writes(tmp_path: Path) -> None:
    template_pdf = make_unstructured_pdf(
        tmp_path / "unstructured.pdf", page_texts=["No numbered questions here."]
    )
    output_dir = (tmp_path / "setup_out").resolve()

    result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(template_pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "unsupported_layout"
    assert not (output_dir / "output_metadata.json").exists()


def test_exam_setup_extracts_max_points_from_cover_page_score_table(tmp_path: Path) -> None:
    template_pdf = make_template_pdf_with_cover_score_table(
        tmp_path / "cover_table_template.pdf",
        questions=[
            TemplateQuestionSpec(number=1, text="What is 2 + 2?", points=5, page_number=2, y=120),
            TemplateQuestionSpec(
                number=2, text="Name a sorting algorithm.", points=3, page_number=2, y=260
            ),
        ],
    )
    output_dir = (tmp_path / "setup_out").resolve()

    result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(template_pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    questions = result.envelope.data["questions"]
    assert [question["question_id"] for question in questions] == ["q1", "q2"]
    assert [question["max_points"] for question in questions] == [5, 3]


def test_exam_setup_extracts_max_points_from_cover_page_point_distribution(tmp_path: Path) -> None:
    template_pdf = make_template_pdf_with_cover_point_distribution(
        tmp_path / "cover_distribution_template.pdf",
        questions=[
            TemplateQuestionSpec(
                number=1, text="Explain westward expansion.", points=8, page_number=2, y=120
            ),
            TemplateQuestionSpec(
                number=2, text="Describe Jacksonian democracy.", points=8, page_number=2, y=260
            ),
            TemplateQuestionSpec(
                number=3,
                text="Compare abolition and women's rights.",
                points=6,
                page_number=2,
                y=400,
            ),
        ],
    )
    output_dir = (tmp_path / "setup_out").resolve()

    result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(template_pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    questions = result.envelope.data["questions"]
    assert [question["question_id"] for question in questions] == ["q1", "q2", "q3"]
    assert [question["max_points"] for question in questions] == [8, 8, 6]


def test_exam_setup_extracts_max_points_from_alternate_cover_page_summary_formats(
    tmp_path: Path,
) -> None:
    template_pdf = (tmp_path / "cover_variants_template.pdf").resolve()
    document = fitz.open()
    try:
        document.new_page()
        document.new_page()
        cover = document.load_page(0)
        cover_lines = [
            "US History Assessment",
            "Point Distribution",
            "Q1: 8 pts",
            "Question 2 (6 points)",
            "3. 4 points",
        ]
        for index, line in enumerate(cover_lines):
            cover.insert_text((72, 72 + (index * 20)), line, fontsize=12)
        question_page = document.load_page(1)
        question_page.insert_text((72, 120), "1. Explain westward expansion.", fontsize=12)
        question_page.insert_text((72, 260), "2. Describe Jacksonian democracy.", fontsize=12)
        question_page.insert_text((72, 400), "3. Compare reform movements.", fontsize=12)
        document.save(template_pdf)
    finally:
        document.close()

    output_dir = (tmp_path / "setup_out").resolve()
    result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(template_pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    questions = result.envelope.data["questions"]
    assert [question["max_points"] for question in questions] == [8, 6, 4]


def test_exam_setup_detects_headers_with_parenthetical_points_on_question_pages(
    tmp_path: Path,
) -> None:
    template_pdf = (tmp_path / "parenthetical_header_template.pdf").resolve()
    document = fitz.open()
    try:
        page = document.new_page()
        page.insert_text((72, 120), "Question 1 (6 points)", fontsize=12)
        page.insert_text((72, 145), "Explain westward expansion.", fontsize=12)
        page.insert_text((72, 260), "Question 2 (4 points)", fontsize=12)
        page.insert_text((72, 285), "Describe Jacksonian democracy.", fontsize=12)
        document.save(template_pdf)
    finally:
        document.close()

    output_dir = (tmp_path / "setup_out").resolve()
    result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(template_pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    questions = result.envelope.data["questions"]
    assert [question["question_id"] for question in questions] == ["q1", "q2"]
    assert [question["max_points"] for question in questions] == [6, 4]


def test_exam_setup_max_points_ignore_bonus_text_above_padded_crop(tmp_path: Path) -> None:
    template_pdf = (tmp_path / "bonus_note_template.pdf").resolve()
    document = fitz.open()
    try:
        page = document.new_page()
        page.insert_text((72, 120), "1. First question (5 points)", fontsize=12)
        page.insert_text((72, 145), "Explain stacks and queues.", fontsize=12)
        page.insert_text((72, 235), "bonus note (1 point)", fontsize=12)
        page.insert_text((72, 260), "Question 2 (6 points)", fontsize=12)
        page.insert_text((72, 285), "Describe binary search trees.", fontsize=12)
        document.save(template_pdf)
    finally:
        document.close()

    output_dir = (tmp_path / "setup_out").resolve()
    result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(template_pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    questions = {
        question["question_id"]: question for question in result.envelope.data["questions"]
    }
    assert questions["q1"]["max_points"] == 5
    assert questions["q2"]["max_points"] == 6


def test_exam_setup_regions_pad_above_current_prompt_and_stop_before_next_prompt(
    tmp_path: Path,
) -> None:
    template_pdf = make_template_pdf(
        tmp_path / "template.pdf",
        questions=[
            TemplateQuestionSpec(number=1, text="What is 2 + 2?", points=5, y=120),
            TemplateQuestionSpec(number=2, text="Name a sorting algorithm.", points=3, y=260),
            TemplateQuestionSpec(number=3, text="Explain stable sorting.", points=4, y=400),
        ],
    )
    output_dir = (tmp_path / "setup_out").resolve()

    result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(template_pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    questions = {
        question["question_number"]: question for question in result.envelope.data["questions"]
    }

    document = open_pdf_document(template_pdf)
    try:
        markers = collapse_markers(
            [marker for page in document for marker in detect_question_markers(page)]
        )
    finally:
        document.close()

    for index, marker in enumerate(markers[:-1], start=1):
        next_marker = markers[index]
        question = questions[marker.question_number]
        region = question["region"]
        region_top = region["y"]
        region_bottom = region["y"] + region["height"]
        current_marker_top = round(marker.rect.y0 * PDF_RENDER_SCALE)
        next_marker_top = round(next_marker.rect.y0 * PDF_RENDER_SCALE)
        assert region_top < current_marker_top
        assert region_bottom < next_marker_top


def test_exam_setup_baseline_pdf_text_ignores_lines_above_padded_crop(tmp_path: Path) -> None:
    template_pdf = (tmp_path / "template_with_instruction_gap.pdf").resolve()
    document = fitz.open()
    try:
        page = document.new_page()
        page.insert_text((72, 120), "1. First question (5 points)", fontsize=12)
        page.insert_text((72, 240), "Answer both parts before continuing.", fontsize=12)
        page.insert_text((72, 260), "2. Second question (4 points)", fontsize=12)
        document.save(template_pdf)
    finally:
        document.close()

    output_dir = (tmp_path / "setup_out").resolve()
    result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(template_pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    questions = {
        question["question_id"]: question for question in result.envelope.data["questions"]
    }
    assert "Second question" in questions["q2"]["baseline_pdf_text"]
    assert "Answer both parts before continuing." not in questions["q2"]["baseline_pdf_text"]


def test_exam_analyze_uses_cli_derived_question_text_and_empty_baseline_warns(
    tmp_path: Path,
) -> None:
    template_png = (tmp_path / "q1.png").resolve()
    template_png.write_bytes(b"")
    captured: dict[str, str] = {}

    def responder(request: LlmRequest) -> LlmResponse:
        if request.prompt_id == "question_text":
            return LlmResponse(
                raw_text="1. Derived clean text",
                provider_response={"model": "qwen-test"},
                usage=LlmTokenUsage(
                    input_tokens=20,
                    output_tokens=5,
                    total_tokens=25,
                    count_source="exact",
                    image_input_count=1,
                ),
            )
        if request.prompt_id == "question_context":
            captured["rendered_text"] = request.rendered_text
            return LlmResponse(
                raw_text="Reference table showing x and y values",
                provider_response={"model": "qwen-test"},
                usage=LlmTokenUsage(
                    input_tokens=30,
                    output_tokens=8,
                    total_tokens=38,
                    count_source="estimated",
                    estimation_method="chars_div_4",
                ),
            )
        raise AssertionError(f"unexpected prompt id: {request.prompt_id}")

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "exam.analyze",
        {
            "question_targets": [
                {
                    "question_id": "q1",
                    "template_question_png_path": str(template_png),
                    "baseline_pdf_text": "",
                }
            ],
            "output_artifacts_dir": str((tmp_path / "analyze_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["question_results"][0]
    assert row["status"] == "ok"
    assert row["question_text_clean"] == "Derived clean text"
    assert row["question_context"] == "Reference table showing x and y values"
    assert row["warnings"][0]["code"] == "baseline_pdf_text_missing"
    assert (
        "<question_text_clean>\nDerived clean text\n</question_text_clean>"
        in captured["rendered_text"]
    )
    assert result.envelope.usage is not None
    assert result.envelope.usage.llm is not None
    usage = result.envelope.usage.llm
    assert usage.input_tokens == 50
    assert usage.output_tokens == 13
    assert usage.total_tokens == 63
    assert usage.count_source == "mixed"
    assert [(call.prompt_id, call.step, call.count_source) for call in usage.calls] == [
        ("question_text", "analyze_question_text", "exact"),
        ("question_context", "analyze_question_context", "estimated"),
    ]
    trace_dir = Path(result.envelope.data["output_metadata_path"]).parent / "traces"
    trace_steps = sorted(path.stem for path in trace_dir.glob("*.json"))
    assert trace_steps == [
        "analyze_question_context__question_id-q1",
        "analyze_question_text__question_id-q1",
    ]
    question_text_trace = json.loads(
        (trace_dir / "analyze_question_text__question_id-q1.json").read_text(encoding="utf-8")
    )
    assert question_text_trace["response"]["usage"]["input_tokens"] == 20
    assert question_text_trace["response"]["usage"]["image_input_count"] == 1


def test_exam_analyze_invalid_assets_response_is_row_local_degraded(tmp_path: Path) -> None:
    template_one = (tmp_path / "q1.png").resolve()
    template_one.write_bytes(b"")
    template_two = (tmp_path / "q2.png").resolve()
    template_two.write_bytes(b"")

    def responder(request: LlmRequest) -> LlmResponse:
        if request.prompt_id == "question_text":
            if "Question 1" in request.rendered_text:
                return LlmResponse(raw_text="alpha cleaned text")
            return LlmResponse(raw_text="beta cleaned text")
        if request.prompt_id == "question_context":
            if "beta cleaned text" in request.rendered_text:
                raise RuntimeError("Simulated context extraction failure.")
            return LlmResponse(raw_text="Some context for question one")
        raise AssertionError(f"unexpected prompt id: {request.prompt_id}")

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "exam.analyze",
        {
            "question_targets": [
                {
                    "question_id": "q1",
                    "template_question_png_path": str(template_one),
                    "baseline_pdf_text": "Question 1 baseline",
                },
                {
                    "question_id": "q2",
                    "template_question_png_path": str(template_two),
                    "baseline_pdf_text": "Question 2 baseline",
                },
            ],
            "output_artifacts_dir": str((tmp_path / "analyze_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    rows = result.envelope.data["question_results"]
    assert [row["status"] for row in rows] == ["ok", "error"]
    assert rows[1]["warnings"][-1]["code"] == "question_context_failed"


def test_exam_generate_rubric_returns_semantic_warnings_and_trace(tmp_path: Path) -> None:
    def responder(request: LlmRequest) -> LlmResponse:
        assert request.prompt_id == "rubric_generate"
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "criteria": [
                        {
                            "label": "Wrong first label",
                            "points": 1,
                            "partial_credit_guidance": "Award 1 point for any attempt.",
                        },
                        {
                            "label": "Substantive correctness",
                            "points": 1,
                            "partial_credit_guidance": "Award 0 or 1 points for correctness.",
                        },
                    ]
                }
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "exam.generate-rubric",
        {
            "question_id": "q1",
            "max_points": 3,
            "subject": "python",
            "question_text_clean": "Explain list slicing.",
            "question_context": "No additional context.",
            "instructor_profile": {
                "grading_strictness": "balanced",
                "syntax_leniency": "medium",
                "ocr_tolerance": "medium",
                "partial_credit_style": "balanced",
                "feedback_style": "brief",
                "additional_guidance": None,
            },
            "output_artifacts_dir": str((tmp_path / "rubric_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    draft = result.envelope.data["rubric_draft"]
    warning_codes = {warning["code"] for warning in draft["warnings"]}
    assert warning_codes == {"rubric_points_mismatch"}
    assert {warning.code for warning in result.envelope.warnings} == warning_codes
    assert "model_output" in draft
    trace_dir = Path(result.envelope.data["output_metadata_path"]).parent / "traces"
    assert [path.name for path in trace_dir.glob("*.json")] == [
        "generate_rubric__question_id-q1.json"
    ]


def test_exam_generate_rubric_typed_provider_failure_preserves_category(tmp_path: Path) -> None:
    def responder(_request: LlmRequest) -> LlmResponse:
        raise ScriptscoreError(
            code="llm_provider_unavailable",
            message="Synthetic provider outage.",
            category=ErrorCategory.EXTERNAL_DEPENDENCY,
            retryable=True,
            write_state=WriteState.NO_WRITE,
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "exam.generate-rubric",
        {
            "question_id": "q1",
            "max_points": 2,
            "subject": "python",
            "question_text_clean": "Explain list slicing.",
            "question_context": "No additional context.",
            "instructor_profile": {
                "grading_strictness": "balanced",
                "syntax_leniency": "medium",
                "ocr_tolerance": "medium",
                "partial_credit_style": "balanced",
                "feedback_style": "brief",
                "additional_guidance": None,
            },
            "output_artifacts_dir": str((tmp_path / "rubric_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code != 0
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "llm_provider_unavailable"
    assert result.envelope.error.category == "external_dependency"


def test_exam_generate_rubric_uses_shared_escaped_question_context_text(tmp_path: Path) -> None:
    captured: dict[str, str] = {}

    def responder(request: LlmRequest) -> LlmResponse:
        captured["rendered_text"] = request.rendered_text
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "criteria": [
                        {
                            "label": "Attempt credit for any non-blank python-related response",
                            "points": 2,
                            "partial_credit_guidance": "Award 2 points if the student makes a relevant attempt.",
                        }
                    ]
                }
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "exam.generate-rubric",
        {
            "question_id": "q1",
            "max_points": 2,
            "subject": "python",
            "question_text_clean": "Explain list slicing.",
            "question_context": "if x < y && z > 0 and Node</item> & edge",
            "instructor_profile": {
                "grading_strictness": "balanced",
                "syntax_leniency": "medium",
                "ocr_tolerance": "medium",
                "partial_credit_style": "balanced",
                "feedback_style": "brief",
                "additional_guidance": None,
            },
            "output_artifacts_dir": str((tmp_path / "rubric_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert "<question_context>" in captured["rendered_text"]
    assert (
        "if x &lt; y &amp;&amp; z &gt; 0 and Node&lt;/item&gt; &amp; edge"
        in captured["rendered_text"]
    )
    assert "</question_context>" in captured["rendered_text"]
    assert captured["rendered_text"].count("<question_context>") == 1


def test_fake_llm_question_context_defaults_to_empty_string() -> None:
    response = FakeLlmProvider().generate(
        LlmRequest(
            prompt_id="question_context",
            response_mode="plain_text",
            rendered_text="<question_text_clean>Cleaned question text.</question_text_clean>",
            provider_config=LlmProviderConfig(model="fake-model"),
        )
    )

    assert response.raw_text == ""


def test_exam_generate_rubric_warns_on_duplicate_and_overlapping_criteria(tmp_path: Path) -> None:
    def responder(request: LlmRequest) -> LlmResponse:
        if request.prompt_id == "rubric_generate":
            return LlmResponse(
                raw_text=json.dumps(
                    {
                        "criteria": [
                            {
                                "label": "Attempt credit for any non-blank python-related response",
                                "points": 1,
                                "partial_credit_guidance": "Award 1 point if the student makes any non-blank python-related attempt.",
                            },
                            {
                                "label": "Identifies start and stop indices in slicing",
                                "points": 1,
                                "partial_credit_guidance": "Award 1 point when the student identifies start and stop indices in slicing.",
                            },
                            {
                                "label": "Identifies start and stop indices in slicing",
                                "points": 1,
                                "partial_credit_guidance": "Award 1 point when the student identifies start and stop indices in slicing.",
                            },
                            {
                                "label": "Explains slicing uses start and stop indices",
                                "points": 1,
                                "partial_credit_guidance": "Award 1 point when the student explains that slicing uses start and stop indices.",
                            },
                        ]
                    }
                )
            )
        assert request.prompt_id == "rubric_semantic_review"
        assert "<rubric_criteria>" in request.rendered_text
        assert "<candidate_pairs>" in request.rendered_text
        assert "candidate_pairs_json" not in request.rendered_text
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "pair_reviews": [
                        {
                            "left_index": 2,
                            "right_index": 3,
                            "classification": "duplicate",
                            "reason": "The criteria are materially the same requirement.",
                        },
                        {
                            "left_index": 2,
                            "right_index": 4,
                            "classification": "overlap",
                            "reason": "Both criteria focus on the same slicing-boundary evidence.",
                        },
                        {
                            "left_index": 3,
                            "right_index": 4,
                            "classification": "overlap",
                            "reason": "Both criteria focus on the same slicing-boundary evidence.",
                        },
                    ]
                }
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "exam.generate-rubric",
        {
            "question_id": "q1",
            "max_points": 4,
            "subject": "python",
            "question_text_clean": "Explain list slicing.",
            "question_context": "No additional context.",
            "instructor_profile": {
                "grading_strictness": "balanced",
                "syntax_leniency": "medium",
                "ocr_tolerance": "medium",
                "partial_credit_style": "balanced",
                "feedback_style": "brief",
                "additional_guidance": None,
            },
            "output_artifacts_dir": str((tmp_path / "rubric_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    draft = result.envelope.data["rubric_draft"]
    warning_codes = {warning["code"] for warning in draft["warnings"]}
    assert "rubric_duplicate_criterion" in warning_codes
    assert "rubric_potential_overlap" in warning_codes
    assert {warning.code for warning in result.envelope.warnings} == warning_codes
    assert "model_output" in draft


def test_exam_generate_rubric_offsets_semantic_warning_indices_for_minimum_credit(
    tmp_path: Path,
) -> None:
    captured: dict[str, str] = {}

    def responder(request: LlmRequest) -> LlmResponse:
        if request.prompt_id == "rubric_generate":
            return LlmResponse(
                raw_text=json.dumps(
                    {
                        "criteria": [
                            {
                                "label": "Identifies start and stop indices in slicing",
                                "points": 1,
                                "partial_credit_guidance": "Award 1 point when the student identifies start and stop indices in slicing.",
                            },
                            {
                                "label": "Identifies start and stop indices in slicing",
                                "points": 1,
                                "partial_credit_guidance": "Award 1 point when the student identifies start and stop indices in slicing.",
                            },
                        ]
                    }
                )
            )
        assert request.prompt_id == "rubric_semantic_review"
        captured["rendered_text"] = request.rendered_text
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "pair_reviews": [
                        {
                            "left_index": 2,
                            "right_index": 3,
                            "classification": "duplicate",
                            "reason": "The two displayed criteria duplicate the same slicing evidence.",
                        }
                    ]
                }
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "exam.generate-rubric",
        {
            "question_id": "q1",
            "max_points": 2,
            "subject": "python",
            "question_text_clean": "Explain list slicing.",
            "question_context": "No additional context.",
            "host_prepends_minimum_credit_criterion": True,
            "instructor_profile": {
                "grading_strictness": "balanced",
                "syntax_leniency": "medium",
                "ocr_tolerance": "medium",
                "partial_credit_style": "balanced",
                "feedback_style": "brief",
                "additional_guidance": None,
            },
            "output_artifacts_dir": str((tmp_path / "rubric_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    draft = result.envelope.data["rubric_draft"]
    warning_scopes = [warning["scope"] for warning in draft["warnings"]]
    assert [scope["criteria"] for scope in warning_scopes] == [[2, 3]]
    assert 'index="2"' in captured["rendered_text"]
    assert 'index="3"' in captured["rendered_text"]
    assert 'left_index="2"' in captured["rendered_text"]
    assert 'right_index="3"' in captured["rendered_text"]


def test_exam_generate_rubric_secondary_review_can_suppress_local_false_positive(
    tmp_path: Path,
) -> None:
    def responder(request: LlmRequest) -> LlmResponse:
        if request.prompt_id == "rubric_generate":
            return LlmResponse(
                raw_text=json.dumps(
                    {
                        "criteria": [
                            {
                                "label": "Attempt credit for any non-blank python-related response",
                                "points": 1,
                                "partial_credit_guidance": "Award 1 point if the student makes any non-blank python-related attempt.",
                            },
                            {
                                "label": "Identifies slice bounds",
                                "points": 1,
                                "partial_credit_guidance": "Award 1 point when the student identifies the slice start and stop values.",
                            },
                            {
                                "label": "Explains omitted bounds behavior",
                                "points": 1,
                                "partial_credit_guidance": "Award 1 point when the student explains what missing start or stop values mean.",
                            },
                        ]
                    }
                )
            )
        assert request.prompt_id == "rubric_semantic_review"
        assert "<rubric_criteria>" in request.rendered_text
        assert "<candidate_pairs>" in request.rendered_text
        assert "candidate_pairs_json" not in request.rendered_text
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "pair_reviews": [
                        {
                            "left_index": 2,
                            "right_index": 3,
                            "classification": "distinct",
                            "reason": "The criteria target different slicing evidence.",
                        }
                    ]
                }
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "exam.generate-rubric",
        {
            "question_id": "q1",
            "max_points": 3,
            "subject": "python",
            "question_text_clean": "Explain list slicing.",
            "question_context": "No additional context.",
            "instructor_profile": {
                "grading_strictness": "balanced",
                "syntax_leniency": "medium",
                "ocr_tolerance": "medium",
                "partial_credit_style": "balanced",
                "feedback_style": "brief",
                "additional_guidance": None,
            },
            "output_artifacts_dir": str((tmp_path / "rubric_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is False
    draft = result.envelope.data["rubric_draft"]
    assert draft["warnings"] == []

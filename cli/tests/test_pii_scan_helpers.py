# SPDX-License-Identifier: AGPL-3.0-only
"""Focused coverage for local PII scan helper modules."""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np
import pytest
from PIL import Image, ImageDraw

from scriptscore.pii_scan import engine
from scriptscore.pii_scan.images import prepare_crop, region_ink_fraction, text_structure_score
from scriptscore.pii_scan.matching import _normalized_phone, detect_student_pii
from scriptscore.pii_scan.reader import PaddleTextReader, create_reader, verify_model_root
from scriptscore.pii_scan.types import RasterBundle, ReadResult, ScanRuntimeOptions, VisionToken


def _fake_model_root(root: Path) -> Path:
    model_root = (root / "models" / "paddle").resolve()
    for leaf in ("det", "rec"):
        target = model_root / leaf
        target.mkdir(parents=True, exist_ok=True)
        (target / "inference.yml").write_text("test", encoding="utf-8")
        (target / "inference.json").write_text("{}", encoding="utf-8")
    return model_root


def _raster(
    *,
    width: int = 240,
    height: int = 120,
    dark_rects: list[tuple[int, int, int, int]] | None = None,
) -> RasterBundle:
    color_bgr = np.full((height, width, 3), 255, dtype=np.uint8)
    grayscale = np.full((height, width), 255, dtype=np.uint8)
    binary_mask = np.full((height, width), 255, dtype=np.uint8)
    for left, top, right, bottom in dark_rects or []:
        grayscale[top:bottom, left:right] = 0
        binary_mask[top:bottom, left:right] = 0
        color_bgr[top:bottom, left:right] = 0
    return RasterBundle(
        path=Path("synthetic.png"),
        color_bgr=color_bgr,
        grayscale=grayscale,
        binary_mask=binary_mask,
        original_size=(width, height),
        working_size=(width, height),
        resize_ratio=1.0,
    )


def _token(
    text: str,
    *,
    left: int,
    top: int,
    right: int,
    bottom: int,
    confidence: float = 0.9,
) -> VisionToken:
    return VisionToken(
        text=text,
        confidence=confidence,
        corners=((left, top), (right, top), (right, bottom), (left, bottom)),
    )


class _Reader:
    def __init__(self, tokens: list[VisionToken], *, backend_name: str = "test_reader") -> None:
        self._tokens = tokens
        self._backend_name = backend_name

    def read(self, image: object) -> ReadResult:
        del image
        return ReadResult(tokens=self._tokens, backend_name=self._backend_name)


class _FailingReader:
    def read(self, image: object) -> ReadResult:
        del image
        raise RuntimeError("ocr exploded")


def test_prepare_crop_flattens_rgba_and_downscales(tmp_path: Path) -> None:
    path = tmp_path / "rgba.png"
    image = Image.new("RGBA", (40, 20), color=(0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    draw.rectangle((30, 8, 34, 12), fill=(0, 0, 0, 255))
    image.save(path, format="PNG")

    raster = prepare_crop(path, max_dimension=20)

    assert raster.original_size == (40, 20)
    assert raster.working_size == (20, 10)
    assert raster.resize_ratio == 0.5
    assert np.asarray(raster.color_bgr).shape == (10, 20, 3)


def test_text_structure_score_and_region_ink_fraction() -> None:
    blank = _raster()
    inked = _raster(dark_rects=[(10, 10, 60, 15), (10, 25, 90, 31)])

    assert text_structure_score(blank) < text_structure_score(inked)
    assert region_ink_fraction(inked.binary_mask, left=-10, top=0, right=20, bottom=20) > 0
    assert region_ink_fraction(inked.binary_mask, left=90, top=90, right=80, bottom=100) == 0.0
    assert region_ink_fraction(inked.binary_mask, left=500, top=0, right=510, bottom=10) == 0.0


def test_verify_model_root_reports_missing_layout(tmp_path: Path) -> None:
    with pytest.raises(RuntimeError, match="missing PaddleOCR det model directory"):
        verify_model_root(tmp_path / "missing")

    model_root = tmp_path / "models"
    (model_root / "det").mkdir(parents=True)
    (model_root / "det" / "inference.yml").write_text("test", encoding="utf-8")
    (model_root / "rec").mkdir(parents=True)
    (model_root / "rec" / "inference.yml").write_text("test", encoding="utf-8")
    (model_root / "rec" / "inference.json").write_text("{}", encoding="utf-8")

    with pytest.raises(RuntimeError, match="missing inference.json or inference.pdmodel"):
        verify_model_root(model_root)


def test_test_override_tokens_validate_and_normalize(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    model_root = _fake_model_root(tmp_path)
    monkeypatch.setenv(
        "SCRIPTSCORE_TEST_PII_OCR_WORDS",
        json.dumps(
            [
                {
                    "text": " Alice ",
                    "confidence": 1.5,
                    "left": 1,
                    "top": 2,
                    "right": 11,
                    "bottom": 12,
                },
                "ignored",
            ]
        ),
    )

    result = create_reader(model_root).read(object())

    assert result.backend_name == "paddleocr_test_override"
    assert len(result.tokens) == 1
    assert result.tokens[0].text == "Alice"
    assert result.tokens[0].confidence == 1.0
    assert result.tokens[0].corners == ((1, 2), (11, 2), (11, 12), (1, 12))

    monkeypatch.setenv("SCRIPTSCORE_TEST_PII_OCR_WORDS", json.dumps({"text": "Alice"}))
    with pytest.raises(RuntimeError, match="must decode to a list"):
        create_reader(model_root).read(object())

    monkeypatch.setenv("SCRIPTSCORE_TEST_PII_OCR_WORDS", "{")
    with pytest.raises(json.JSONDecodeError):
        create_reader(model_root).read(object())


def test_paddle_reader_normalizes_dict_and_legacy_payloads(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_PII_OCR_WORDS", raising=False)

    class _EmptyPageEngine:
        def ocr(self, _image: object) -> list[None]:
            return [None]

    class _DictEngine:
        def ocr(self, _image: object) -> list[dict[str, object]]:
            return [
                {
                    "rec_texts": [" Alice ", ""],
                    "rec_scores": [1.2, 0.5],
                    "rec_polys": [
                        [(1, 2), (11, 2), (11, 12), (1, 12)],
                        [(0, 0), (1, 0), (1, 1), (0, 1)],
                    ],
                }
            ]

    dict_reader = object.__new__(PaddleTextReader)
    dict_reader._engine = _DictEngine()
    dict_result = dict_reader.read(np.full((8, 8), 255, dtype=np.uint8))

    assert [(token.text, token.confidence) for token in dict_result.tokens] == [("Alice", 1.0)]

    empty_page_reader = object.__new__(PaddleTextReader)
    empty_page_reader._engine = _EmptyPageEngine()
    assert empty_page_reader.read(np.full((8, 8, 3), 255, dtype=np.uint8)).tokens == []

    class _LegacyEngine:
        def ocr(
            self, _image: object
        ) -> list[list[tuple[list[tuple[int, int]], tuple[str, float]]]]:
            return [
                [
                    ([(3, 4), (13, 4), (13, 14), (3, 14)], ("Bob", -0.5)),
                    ([(0, 0), (1, 0)], ("bad", 0.9)),
                ]
            ]

    legacy_reader = object.__new__(PaddleTextReader)
    legacy_reader._engine = _LegacyEngine()
    legacy_result = legacy_reader.read(np.full((8, 8, 3), 255, dtype=np.uint8))

    assert [(token.text, token.confidence) for token in legacy_result.tokens] == [("Bob", 0.0)]


def test_detect_student_pii_matches_patterns_labels_names_and_dedupes() -> None:
    raster = _raster(dark_rects=[(55, 18, 130, 24)])
    tokens = [
        _token("Question", left=0, top=0, right=48, bottom=10),
        _token("Score", left=55, top=0, right=85, bottom=10),
        _token("Maximum", left=90, top=0, right=140, bottom=10),
        _token("Name:", left=5, top=15, right=50, bottom=28),
        _token("User", left=5, top=42, right=35, bottom=54),
        _token("ID", left=40, top=42, right=52, bottom=54),
        _token("asmith42", left=60, top=42, right=120, bottom=54),
        _token("Alice", left=5, top=70, right=45, bottom=84),
        _token("Smyth", left=50, top=70, right=90, bottom=84),
        _token("Alice", left=5, top=96, right=45, bottom=110),
        _token("Smith", left=50, top=96, right=90, bottom=110),
    ]
    extracted = "Email: alice@example.edu alice@example.edv Call 202-555-1254 @asmith42"

    hits = detect_student_pii(
        extracted_text=extracted,
        tokens=tokens,
        raster=raster,
        trigger_words=[
            "Alice Smith",
            "alice@example.edu",
            "2025551254",
            "asmith42",
        ],
    )

    assert _normalized_phone("2O2-555-I2S4") == "2025551254"
    assert [(hit.kind, hit.snippet) for hit in hits] == [
        ("email", "alice@example.edu"),
        ("phone_number", "202-555-1254"),
        ("email", "alice@example.edv"),
        ("username", "@asmith42"),
        ("username", "asmith42"),
        ("name", "Alice Smith"),
        ("name", "Alice Smyth"),
        ("name", ""),
    ]


def test_inspect_student_crop_reports_preprocessing_failure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    def prepare_failure(_path: Path, *, max_dimension: int) -> RasterBundle:
        raise RuntimeError("bad image")

    monkeypatch.setattr("scriptscore.pii_scan.engine.prepare_crop", prepare_failure)

    finding = engine.inspect_student_crop(
        tmp_path / "missing.png",
        trigger_words=["Alice Smith"],
        options=ScanRuntimeOptions(model_root=tmp_path),
        reader=_Reader([]),
    )

    assert finding.fatal_error == "bad image"
    assert finding.handwriting_state == "unknown"
    assert finding.pii_present is False
    assert finding.stage_durations.keys() == {"preprocess"}


def test_inspect_student_crop_degrades_when_ocr_fails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr("scriptscore.pii_scan.engine.prepare_crop", lambda *_args, **_kw: _raster())
    monkeypatch.setattr("scriptscore.pii_scan.engine.text_structure_score", lambda _raster: 0.2)

    finding = engine.inspect_student_crop(
        tmp_path / "crop.png",
        trigger_words=["Alice Smith"],
        options=ScanRuntimeOptions(model_root=tmp_path),
        reader=_FailingReader(),
    )

    assert finding.handwriting_state == "unknown"
    assert finding.backend_warnings == ["OCR failed: ocr exploded"]
    assert finding.metrics["backend_name"] == "paddleocr"


def test_inspect_student_crop_distinguishes_blank_and_text_like_no_ocr(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr("scriptscore.pii_scan.engine.prepare_crop", lambda *_args, **_kw: _raster())
    monkeypatch.setattr("scriptscore.pii_scan.engine.text_structure_score", lambda _raster: 0.01)
    blank = engine.inspect_student_crop(
        tmp_path / "blank.png",
        trigger_words=["Alice Smith"],
        options=ScanRuntimeOptions(model_root=tmp_path),
        reader=_Reader([]),
    )

    monkeypatch.setattr("scriptscore.pii_scan.engine.text_structure_score", lambda _raster: 0.2)
    unknown = engine.inspect_student_crop(
        tmp_path / "unknown.png",
        trigger_words=["Alice Smith"],
        options=ScanRuntimeOptions(model_root=tmp_path),
        reader=_Reader([]),
    )

    assert blank.handwriting_state == "false"
    assert blank.pii_present is False
    assert unknown.handwriting_state == "unknown"
    assert unknown.pii_present is False


def test_inspect_student_crop_suppresses_pii_when_printed_text_is_not_handwriting(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raster = _raster(width=260, height=120)
    tokens = [
        _token("Question", left=5, top=3, right=45, bottom=13, confidence=0.94),
        _token("Email", left=50, top=3, right=80, bottom=13, confidence=0.93),
        _token("Alice", left=85, top=3, right=115, bottom=13, confidence=0.95),
        _token("alice@example.edu", left=120, top=3, right=210, bottom=13, confidence=0.96),
        _token("Score", left=5, top=16, right=35, bottom=26, confidence=0.95),
        _token("Maximum", left=40, top=16, right=88, bottom=26, confidence=0.94),
    ]
    monkeypatch.setattr("scriptscore.pii_scan.engine.prepare_crop", lambda *_args, **_kw: raster)
    monkeypatch.setattr("scriptscore.pii_scan.engine.text_structure_score", lambda _raster: 0.05)

    finding = engine.inspect_student_crop(
        tmp_path / "printed.png",
        trigger_words=["alice@example.edu"],
        options=ScanRuntimeOptions(model_root=tmp_path, include_text=False, include_metrics=False),
        reader=_Reader(tokens),
    )

    assert finding.handwriting_state == "false"
    assert finding.pii_present is False
    assert finding.pii_kinds == []
    assert "suppressed because handwriting was not confidently present" in " ".join(finding.reasons)
    assert finding.metrics == {}
    assert finding.extracted_text is None


def test_inspect_student_crop_exposes_metrics_and_text_when_requested(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raster = _raster(dark_rects=[(10, 70, 190, 82), (15, 90, 120, 98)])
    tokens = [
        _token("Alice", left=10, top=70, right=45, bottom=88, confidence=0.55),
        _token("Smith", left=50, top=70, right=92, bottom=88, confidence=0.52),
    ]
    monkeypatch.setattr("scriptscore.pii_scan.engine.prepare_crop", lambda *_args, **_kw: raster)
    monkeypatch.setattr("scriptscore.pii_scan.engine.text_structure_score", lambda _raster: 0.3)

    finding = engine.inspect_student_crop(
        tmp_path / "handwriting.png",
        trigger_words=["Alice Smith"],
        options=ScanRuntimeOptions(model_root=tmp_path, include_text=True, include_metrics=True),
        reader=_Reader(tokens),
    )

    assert finding.handwriting_state in {"true", "unknown"}
    assert finding.extracted_text == "Alice Smith"
    assert set(finding.stage_durations) == {"preprocess", "ocr", "handwriting", "pii"}
    assert finding.metrics["backend_name"] == "test_reader"
    if finding.handwriting_state == "true":
        assert finding.pii_present is True
        assert finding.pii_kinds == ["name"]

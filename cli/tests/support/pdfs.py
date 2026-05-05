# SPDX-License-Identifier: AGPL-3.0-only
"""Deterministic PDF fixtures for Phase 3 command tests."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import fitz  # type: ignore[import-untyped]


@dataclass(frozen=True)
class TemplateQuestionSpec:
    """One template-question text block."""

    number: int
    text: str
    points: int
    page_number: int = 1
    y: float = 120.0


def make_student_pdf(path: Path, *, page_texts: list[str]) -> Path:
    """Create a simple student PDF with one text block per page."""

    path.parent.mkdir(parents=True, exist_ok=True)
    document = fitz.open()
    try:
        for text in page_texts:
            page = document.new_page()
            page.insert_text((72, 120), text, fontsize=12)
        document.save(path)
    finally:
        document.close()
    return path.resolve()


def make_template_pdf(path: Path, *, questions: list[TemplateQuestionSpec]) -> Path:
    """Create a simple template PDF with numbered question text blocks."""

    path.parent.mkdir(parents=True, exist_ok=True)
    document = fitz.open()
    try:
        max_page = max(question.page_number for question in questions)
        for _ in range(max_page):
            document.new_page()
        for question in questions:
            page = document.load_page(question.page_number - 1)
            page.insert_text((72, question.y), f"{question.number}. {question.text}", fontsize=12)
            page.insert_text((460, question.y), f"{question.points} pts", fontsize=12)
        document.save(path)
    finally:
        document.close()
    return path.resolve()


def make_template_pdf_with_cover_score_table(
    path: Path, *, questions: list[TemplateQuestionSpec]
) -> Path:
    """Create a template PDF with a cover-page score table and question pages."""

    path.parent.mkdir(parents=True, exist_ok=True)
    document = fitz.open()
    try:
        max_page = max(question.page_number for question in questions)
        for _ in range(max_page):
            document.new_page()
        cover = document.load_page(0)
        cover.insert_text((72, 120), "Question", fontsize=12)
        cover.insert_text((160, 120), "Score", fontsize=12)
        cover.insert_text((220, 120), "Maximum", fontsize=12)
        for index, question in enumerate(sorted(questions, key=lambda item: item.number), start=1):
            y = 140 + ((index - 1) * 22)
            cover.insert_text((72, y), str(question.number), fontsize=12)
            cover.insert_text((220, y), str(question.points), fontsize=12)
        cover.insert_text((72, 140 + (len(questions) * 22)), "Total", fontsize=12)
        cover.insert_text(
            (220, 140 + (len(questions) * 22)),
            str(sum(question.points for question in questions)),
            fontsize=12,
        )

        for question in questions:
            page = document.load_page(question.page_number - 1)
            page.insert_text((72, question.y), f"{question.number}. {question.text}", fontsize=12)
        document.save(path)
    finally:
        document.close()
    return path.resolve()


def make_template_pdf_with_cover_point_distribution(
    path: Path, *, questions: list[TemplateQuestionSpec]
) -> Path:
    """Create a template PDF with a prose point-distribution overview on the first page."""

    path.parent.mkdir(parents=True, exist_ok=True)
    document = fitz.open()
    try:
        max_page = max(question.page_number for question in questions)
        for _ in range(max_page):
            document.new_page()
        cover = document.load_page(0)
        cover_lines = [
            "US History Assessment",
            "Name: ________________________________",
            "Assessment Overview:",
            "You will answer short essay questions (3-6 sentences each) covering:",
            "- Economic transformation (transportation and industry)",
            "- Expansion of democracy under Andrew Jackson",
            "- Social reform movements (abolition and women's rights)",
            f"Point Distribution (Total: {sum(question.points for question in questions)} points):",
        ]
        cover_lines.extend(
            f"Question {question.number} - {question.points} points"
            for question in sorted(questions, key=lambda item: item.number)
        )
        for index, line in enumerate(cover_lines):
            cover.insert_text((72, 72 + (index * 20)), line, fontsize=12)

        for question in questions:
            page = document.load_page(question.page_number - 1)
            page.insert_text((72, question.y), f"{question.number}. {question.text}", fontsize=12)
        document.save(path)
    finally:
        document.close()
    return path.resolve()


def make_unstructured_pdf(path: Path, *, page_texts: list[str]) -> Path:
    """Create a PDF without supported question markers."""

    return make_student_pdf(path, page_texts=page_texts)

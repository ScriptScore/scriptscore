# SPDX-License-Identifier: AGPL-3.0-only
"""Compatibility exports for exam command specs."""

from scriptscore.commands.exam_analyze import exam_analyze_spec, handle_exam_analyze
from scriptscore.commands.exam_generate_rubric import (
    exam_generate_rubric_spec,
    handle_exam_generate_rubric,
)
from scriptscore.commands.exam_setup import exam_setup_spec, handle_exam_setup

__all__ = [
    "exam_analyze_spec",
    "exam_generate_rubric_spec",
    "exam_setup_spec",
    "handle_exam_analyze",
    "handle_exam_generate_rubric",
    "handle_exam_setup",
]

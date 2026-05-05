# SPDX-License-Identifier: AGPL-3.0-only
"""Internal OCR helpers."""

from scriptscore.ocr.easyocr import OcrTextBox
from scriptscore.ocr.paddle import read_page_ocr

__all__ = ["OcrTextBox", "read_page_ocr"]

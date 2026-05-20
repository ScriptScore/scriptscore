# SPDX-License-Identifier: AGPL-3.0-only
"""OCR backend wrapper for local handwriting and PII scanning."""

from __future__ import annotations

import importlib.metadata
import importlib.util
import json
import os
import sys
import tempfile
import types
from collections.abc import Iterable, Sequence
from pathlib import Path
from typing import Any, Protocol, cast

import cv2
import yaml

from scriptscore.pii_scan.types import ReadResult, VisionToken

_AISTUDIO_PACKAGE = "aistudio_sdk"
_AISTUDIO_SNAPSHOT_DOWNLOAD_MODULE = "aistudio_sdk.snapshot_download"


def _major_version(version: str) -> int | None:
    try:
        return int(version.split(".", 1)[0])
    except ValueError:
        return None


def _is_paddle_3_or_newer(version: str) -> bool:
    major = _major_version(version)
    return major is not None and major >= 3


def _installed_distribution_version(package_name: str) -> str | None:
    try:
        return importlib.metadata.version(package_name)
    except importlib.metadata.PackageNotFoundError:
        return None


def _model_name_from_yaml(model_dir: Path) -> str | None:
    metadata_path = model_dir / "inference.yml"
    if not metadata_path.is_file():
        return None
    try:
        payload = yaml.safe_load(metadata_path.read_text(encoding="utf-8"))
    except yaml.YAMLError as exc:
        raise RuntimeError(f"invalid PaddleOCR model metadata: {metadata_path}: {exc}") from exc
    if not isinstance(payload, dict):
        return None
    global_config = payload.get("Global")
    if not isinstance(global_config, dict):
        return None
    model_name = global_config.get("model_name")
    return str(model_name).strip() if model_name is not None else None


def _model_names(model_root: Path) -> dict[str, str | None]:
    return {name: _model_name_from_yaml(model_root / name) for name in ("det", "rec")}


def _verify_model_family_compatible(
    model_root: Path,
    *,
    paddleocr_version: str | None,
) -> None:
    if paddleocr_version is None:
        return
    major = _major_version(paddleocr_version)
    if major is None:
        return
    names = _model_names(model_root)
    v5_names = {
        role: name
        for role, name in names.items()
        if name is not None and name.startswith("PP-OCRv5")
    }
    if v5_names and major < 3:
        described = ", ".join(f"{role}={name}" for role, name in sorted(v5_names.items()))
        raise RuntimeError(
            "PaddleOCR PP-OCRv5 model resources require PaddleOCR 3.x or newer; "
            f"found paddleocr {paddleocr_version} with {described}"
        )


def _disable_windows_paddle_ir_optim() -> None:
    if sys.platform != "win32":
        return
    import paddle  # type: ignore[import-not-found, import-untyped]
    from paddle import inference  # type: ignore[import-not-found, import-untyped]

    if not _is_paddle_3_or_newer(str(getattr(paddle, "__version__", ""))):
        return
    if getattr(inference.Config.switch_ir_optim, "_scriptscore_windows_patch", False):
        return

    original_switch_ir_optim = cast(Any, inference.Config.switch_ir_optim)

    def switch_ir_optim_off(self: Any, enabled: object) -> None:
        del enabled
        original_switch_ir_optim(self, False)

    switch_ir_optim_off._scriptscore_windows_patch = True  # type: ignore[attr-defined]
    inference.Config.switch_ir_optim = switch_ir_optim_off  # type: ignore[assignment, method-assign]


def _install_aistudio_download_stub() -> None:
    """Satisfy PaddleX's import-time AI Studio hook when local models are used."""

    if (
        _AISTUDIO_SNAPSHOT_DOWNLOAD_MODULE in sys.modules
        or importlib.util.find_spec(_AISTUDIO_PACKAGE) is not None
    ):
        return

    package = types.ModuleType(_AISTUDIO_PACKAGE)
    package.__path__ = []  # type: ignore[attr-defined]
    snapshot_module = types.ModuleType(_AISTUDIO_SNAPSHOT_DOWNLOAD_MODULE)

    def snapshot_download(*_args: object, **_kwargs: object) -> None:
        raise RuntimeError(
            "AI Studio model downloads are disabled in ScriptScore's bundled PaddleOCR runtime; "
            "configure local PaddleOCR model directories instead."
        )

    snapshot_module.snapshot_download = snapshot_download  # type: ignore[attr-defined]
    package.snapshot_download = snapshot_module  # type: ignore[attr-defined]
    sys.modules[_AISTUDIO_PACKAGE] = package
    sys.modules[_AISTUDIO_SNAPSHOT_DOWNLOAD_MODULE] = snapshot_module


def _allow_headless_opencv_for_paddlex_ocr_extra() -> None:
    """Let PaddleX's OCR extra check accept ScriptScore's headless OpenCV wheel."""

    try:
        from paddlex.utils import deps as paddlex_deps  # type: ignore[import-untyped]
    except Exception:
        return

    extras = getattr(paddlex_deps, "EXTRAS", None)
    if not isinstance(extras, dict):
        return

    is_dep_available = getattr(paddlex_deps, "is_dep_available", None)
    if callable(is_dep_available) and not getattr(
        is_dep_available, "_scriptscore_headless_opencv_patch", False
    ):
        original_is_dep_available = is_dep_available

        def patched_is_dep_available(dep: str, /, check_version: bool = False) -> bool:
            if dep == "opencv-contrib-python":
                return bool(
                    original_is_dep_available("opencv-contrib-python-headless", check_version=False)
                )
            return bool(original_is_dep_available(dep, check_version=check_version))

        patched_is_dep_available._scriptscore_headless_opencv_patch = True  # type: ignore[attr-defined]
        patched_is_dep_available.cache_clear = (  # type: ignore[attr-defined]
            getattr(original_is_dep_available, "cache_clear", lambda: None)
        )
        paddlex_deps.is_dep_available = patched_is_dep_available

    for extra_name in ("ocr-core", "ocr", "base"):
        extra = extras.get(extra_name)
        if not isinstance(extra, dict) or "opencv-contrib-python" not in extra:
            continue
        extra.setdefault("opencv-contrib-python-headless", extra["opencv-contrib-python"])
        extra.pop("opencv-contrib-python", None)

    cache_clear = getattr(getattr(paddlex_deps, "is_extra_available", None), "cache_clear", None)
    if callable(cache_clear):
        cache_clear()

    for module_name, module in sys.modules.items():
        if module_name.startswith("paddlex.") and not hasattr(module, "cv2"):
            module.cv2 = cv2  # type: ignore[attr-defined]


def verify_model_root(model_root: Path, *, paddleocr_version: str | None = None) -> None:
    """Validate the expected PaddleOCR model layout."""

    for name in ("det", "rec"):
        model_dir = model_root / name
        if not model_dir.exists() or not model_dir.is_dir():
            raise RuntimeError(f"missing PaddleOCR {name} model directory: {model_dir}")
        if not (
            (model_dir / "inference.json").exists() or (model_dir / "inference.pdmodel").exists()
        ):
            raise RuntimeError(
                "PaddleOCR "
                f"{name} model directory is missing inference.json or inference.pdmodel: {model_dir}"
            )
        if (model_dir / "inference.pdmodel").exists() and not (
            model_dir / "inference.pdiparams"
        ).exists():
            raise RuntimeError(
                f"PaddleOCR {name} model directory is missing inference.pdiparams: {model_dir}"
            )
    _verify_model_family_compatible(
        model_root,
        paddleocr_version=(
            _installed_distribution_version("paddleocr")
            if paddleocr_version is None
            else paddleocr_version
        ),
    )


def _token_from_payload(payload: dict[str, Any]) -> VisionToken:
    return VisionToken(
        text=str(payload["text"]).strip(),
        confidence=max(0.0, min(1.0, float(payload["confidence"]))),
        corners=(
            (int(payload["left"]), int(payload["top"])),
            (int(payload["right"]), int(payload["top"])),
            (int(payload["right"]), int(payload["bottom"])),
            (int(payload["left"]), int(payload["bottom"])),
        ),
    )


def _test_override_tokens() -> list[VisionToken] | None:
    raw_payload = os.environ.get("SCRIPTSCORE_TEST_PII_OCR_WORDS")
    if raw_payload is None:
        return None
    decoded = json.loads(raw_payload)
    if not isinstance(decoded, list):
        raise RuntimeError("SCRIPTSCORE_TEST_PII_OCR_WORDS must decode to a list.")
    return [_token_from_payload(item) for item in decoded if isinstance(item, dict)]


class _TestOverrideReader:
    """Lightweight OCR reader used by test-only env overrides."""

    def read(self, image: object) -> ReadResult:
        del image
        return ReadResult(
            tokens=_test_override_tokens() or [], backend_name="paddleocr_test_override"
        )


class TextReader(Protocol):
    """OCR reader interface shared by production and test-only readers."""

    def read(self, image: object) -> ReadResult: ...


class PaddleTextReader:
    """Small adapter around PaddleOCR that returns normalized tokens."""

    name = "paddleocr"

    def __init__(self, model_root: Path) -> None:
        verify_model_root(model_root)
        if importlib.util.find_spec("paddle") is None:
            raise RuntimeError(
                "paddleocr backend unavailable: install the 'paddlepaddle' Python package"
            )

        os.environ["DISABLE_MODEL_SOURCE_CHECK"] = "True"
        os.environ["PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK"] = "True"
        os.environ.setdefault(
            "PADDLE_PDX_CACHE_HOME",
            str(Path(tempfile.gettempdir()) / "scriptscore-paddlex-cache"),
        )
        # On Windows, Paddle can load DLLs before Torch in a way that makes
        # Torch's shm.dll import fail. Importing Torch first avoids that clash.
        if importlib.util.find_spec("torch") is not None:
            import torch  # noqa: F401  # type: ignore[import-untyped]

        _disable_windows_paddle_ir_optim()
        _install_aistudio_download_stub()

        from paddleocr import PaddleOCR  # type: ignore[import-untyped]

        _allow_headless_opencv_for_paddlex_ocr_extra()

        model_names = _model_names(model_root)
        self._engine = PaddleOCR(
            text_detection_model_name=model_names["det"] or "PP-OCRv5_mobile_det",
            text_detection_model_dir=str(model_root / "det"),
            text_recognition_model_name=model_names["rec"] or "PP-OCRv5_mobile_rec",
            text_recognition_model_dir=str(model_root / "rec"),
            use_doc_orientation_classify=False,
            use_doc_unwarping=False,
            use_textline_orientation=False,
            enable_mkldnn=False,
            device="cpu",
        )

    def read(self, image: object) -> ReadResult:
        test_tokens = _test_override_tokens()
        if test_tokens is not None:
            return ReadResult(tokens=test_tokens, backend_name="paddleocr_test_override")

        if hasattr(image, "shape") and len(image.shape) == 2:
            grayscale_image = cast("cv2.typing.MatLike", image)
            image = cv2.cvtColor(grayscale_image, cv2.COLOR_GRAY2BGR)

        tokens: list[VisionToken] = []
        for payload in _prediction_payloads(self._engine.predict(input=image)):
            tokens.extend(_tokens_from_prediction_payload(payload))
        return ReadResult(tokens=tokens, backend_name=self.name)


def _prediction_payloads(result: object) -> list[object]:
    if result is None:
        return []
    if isinstance(result, dict):
        return [result.get("res", result)]
    if isinstance(result, list | tuple):
        payloads: list[object] = []
        for item in result:
            payloads.extend(_prediction_payloads(item))
        return payloads
    result_object = cast(Any, result)
    if hasattr(result, "json"):
        json_payload = result_object.json
        if callable(json_payload):
            json_payload = json_payload()
        if isinstance(json_payload, dict):
            return _prediction_payloads(json_payload)
    if hasattr(result, "res"):
        return _prediction_payloads(result_object.res)
    try:
        return _prediction_payloads(result_object["res"])
    except Exception:
        return []


def _tokens_from_prediction_payload(payload: object) -> list[VisionToken]:
    if not isinstance(payload, dict):
        return []
    texts = _payload_sequence(payload, "rec_texts")
    scores = _payload_sequence(payload, "rec_scores")
    polygons = _payload_sequence(payload, "rec_polys")
    tokens: list[VisionToken] = []
    for text, score, polygon in zip(texts, scores, polygons, strict=False):
        token = _normalize_token(text=text, confidence=score, polygon=polygon)
        if token is not None:
            tokens.append(token)
    return tokens


def _payload_sequence(payload: dict[str, object], key: str) -> Iterable[Any]:
    value = payload.get(key)
    if value is None:
        return []
    return cast(Iterable[Any], value)


def _normalize_token(*, text: object, confidence: object, polygon: object) -> VisionToken | None:
    cleaned = str(text).strip()
    if not cleaned:
        return None
    try:
        points = cast(Iterable[Sequence[object]], polygon)
        corners = tuple((int(cast(Any, point[0])), int(cast(Any, point[1]))) for point in points)
    except Exception:
        return None
    if len(corners) != 4:
        return None
    top_left, top_right, bottom_right, bottom_left = corners
    return VisionToken(
        text=cleaned,
        confidence=max(0.0, min(1.0, float(cast(Any, confidence)))),
        corners=(top_left, top_right, bottom_right, bottom_left),
    )


def create_reader(model_root: Path) -> TextReader:
    """Create the local OCR reader for scans.pii."""

    if os.environ.get("SCRIPTSCORE_TEST_PII_OCR_WORDS") is not None:
        verify_model_root(model_root)
        return _TestOverrideReader()
    try:
        return PaddleTextReader(model_root)
    except Exception as exc:
        raise RuntimeError(f"paddleocr backend unavailable: {exc}") from exc

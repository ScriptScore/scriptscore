# SPDX-License-Identifier: AGPL-3.0-only
"""Built-in template-matching alignment provider."""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any, Literal

from scriptscore.contracts import ErrorCategory, ScriptscoreError, WarningObject
from scriptscore.providers.aruco import (
    _detect_aruco_markers as _shared_detect_aruco_markers,
)
from scriptscore.providers.aruco import (
    _load_cv_dependencies as _shared_load_cv_dependencies,
)
from scriptscore.providers.aruco import (
    _marker_centers as _shared_marker_centers,
)
from scriptscore.providers.constants import PROVIDER_INTERFACE_VERSION
from scriptscore.providers.interfaces import AlignmentProvider, AlignmentRequest, AlignmentResponse

_detect_aruco_markers = _shared_detect_aruco_markers
_load_cv_dependencies = _shared_load_cv_dependencies
_marker_centers = _shared_marker_centers

_FAST_MATCH_SCALE = 0.5
_TOP_FRACTION = 0.12
_CENTER_TRIM_FRACTION = 0.15
_COARSE_SCALES = [1.0, 0.98, 1.02, 0.96, 1.04, 0.94, 1.06]
_COARSE_ANGLES = [index * 0.5 for index in range(-6, 7)]
_COARSE_EARLY_STOP = 0.80
_FINE_SCALE_STEPS = [-0.01, -0.008, -0.006, -0.004, -0.002, 0.0, 0.002, 0.004, 0.006, 0.008, 0.01]
_FINE_ANGLE_STEPS = [-0.4, -0.3, -0.2, -0.1, 0.0, 0.1, 0.2, 0.3, 0.4]
_FULL_RES_ANGLE_STEPS = [-0.1, 0.0, 0.1]
_STATUS_OK_MIN = 0.55
_STATUS_LOW_CONFIDENCE_MIN = 0.35
_SEARCH_MARGIN_FRACTION = 0.30
_TRANSLATION_PENALTY = 0.0005
_IDENTITY_SCORE_EPSILON = 0.005
_IDENTITY_SNAP_MAX_ROTATION = 0.25
_IDENTITY_SNAP_MAX_SCALE_DELTA = 0.01
_IDENTITY_SNAP_MAX_TRANSLATION = 2.0
_FLOAT_IDENTITY_ABS_TOL = 1e-12


def _is_identity_float(value: float) -> bool:
    return math.isclose(value, 1.0, rel_tol=0.0, abs_tol=_FLOAT_IDENTITY_ABS_TOL)


def _scriptscore_error(
    *,
    code: str,
    message: str,
    details: dict[str, Any],
    retryable: bool,
    category: ErrorCategory,
) -> ScriptscoreError:
    return ScriptscoreError(
        code=code,
        message=message,
        category=category,
        retryable=retryable,
        details=details,
    )


def _provider_error(
    *, code: str, message: str, details: dict[str, Any], retryable: bool = False
) -> ScriptscoreError:
    return _scriptscore_error(
        code=code,
        message=message,
        details=details,
        retryable=retryable,
        category=ErrorCategory.PROVIDER,
    )


def _execution_error(
    *, code: str, message: str, details: dict[str, Any], retryable: bool = False
) -> ScriptscoreError:
    return _scriptscore_error(
        code=code,
        message=message,
        details=details,
        retryable=retryable,
        category=ErrorCategory.EXECUTION,
    )


def _warning(*, code: str, message: str) -> WarningObject:
    return WarningObject(code=code, message=message)


def _marker_fallback_warning() -> WarningObject:
    return _warning(
        code="marker_guided_alignment_not_used",
        message="Marker-guided alignment was not used; template matching fallback was applied.",
    )


def _read_grayscale(path: str, *, cv2: Any) -> Any:
    image = cv2.imread(path, cv2.IMREAD_GRAYSCALE)
    if image is None:
        raise _execution_error(
            code="alignment_image_unreadable",
            message="One or more alignment images could not be decoded.",
            details={"image_path": path},
        )
    return image


def _simulate_transform_point(
    *,
    width: int,
    height: int,
    x: float,
    y: float,
    rotation: float,
    scale: float,
    translate_x: float,
    translate_y: float,
    cv2: Any,
) -> tuple[float, float]:
    center = (width / 2.0, height / 2.0)
    matrix = cv2.getRotationMatrix2D(center, -rotation, 1.0)
    cos = abs(matrix[0, 0])
    sin = abs(matrix[0, 1])
    bound_width = max(1, int((height * sin) + (width * cos)))
    bound_height = max(1, int((height * cos) + (width * sin)))
    matrix[0, 2] += (bound_width / 2.0) - center[0]
    matrix[1, 2] += (bound_height / 2.0) - center[1]
    rotated_x = (matrix[0, 0] * x) + (matrix[0, 1] * y) + matrix[0, 2]
    rotated_y = (matrix[1, 0] * x) + (matrix[1, 1] * y) + matrix[1, 2]
    return (rotated_x * scale) + translate_x, (rotated_y * scale) + translate_y


def _estimate_aruco_alignment(
    *, template: Any, student: Any, cv2: Any, np: Any
) -> AlignmentResponse | None:
    template_markers = _detect_aruco_markers(template, cv2=cv2)
    student_markers = _detect_aruco_markers(student, cv2=cv2)
    shared_ids = sorted(set(template_markers) & set(student_markers))
    if len(shared_ids) < 2:
        return None

    template_centers = _marker_centers(template_markers, np=np)
    student_centers = _marker_centers(student_markers, np=np)
    template_points = np.float32([template_centers[marker_id] for marker_id in shared_ids])
    student_points = np.float32([student_centers[marker_id] for marker_id in shared_ids])
    matrix, _inliers = cv2.estimateAffinePartial2D(student_points, template_points)
    if matrix is None:
        return None

    cos_component = float(matrix[0, 0])
    sin_component = float(matrix[1, 0])
    scale = math.sqrt((cos_component * cos_component) + (sin_component * sin_component))
    if scale <= 0.0:
        return None
    angle_ccw = math.degrees(math.atan2(float(matrix[0, 1]), float(matrix[0, 0])))
    rotation = -angle_ccw

    projected_without_translation = [
        _simulate_transform_point(
            width=student.shape[1],
            height=student.shape[0],
            x=float(student_centers[marker_id][0]),
            y=float(student_centers[marker_id][1]),
            rotation=rotation,
            scale=scale,
            translate_x=0.0,
            translate_y=0.0,
            cv2=cv2,
        )
        for marker_id in shared_ids
    ]
    translate_x = sum(
        float(template_centers[marker_id][0]) - projected_x
        for marker_id, (projected_x, _projected_y) in zip(
            shared_ids, projected_without_translation, strict=False
        )
    ) / len(shared_ids)
    translate_y = sum(
        float(template_centers[marker_id][1]) - projected_y
        for marker_id, (_projected_x, projected_y) in zip(
            shared_ids, projected_without_translation, strict=False
        )
    ) / len(shared_ids)

    errors: list[float] = []
    for marker_id in shared_ids:
        projected_x, projected_y = _simulate_transform_point(
            width=student.shape[1],
            height=student.shape[0],
            x=float(student_centers[marker_id][0]),
            y=float(student_centers[marker_id][1]),
            rotation=rotation,
            scale=scale,
            translate_x=translate_x,
            translate_y=translate_y,
            cv2=cv2,
        )
        dx = float(template_centers[marker_id][0]) - projected_x
        dy = float(template_centers[marker_id][1]) - projected_y
        errors.append(math.hypot(dx, dy))

    mean_error = sum(errors) / len(errors)
    marker_ratio = len(shared_ids) / max(len(template_markers), 1)
    if mean_error < 5.0 and marker_ratio >= 0.66:
        confidence = min(0.90 + (0.03 * len(shared_ids)), 0.99)
    elif mean_error < 15.0 and len(shared_ids) >= 2:
        confidence = 0.75
    else:
        confidence = 0.50
    status = _classify_status(confidence)
    return AlignmentResponse(
        status=status,
        confidence=confidence,
        rotation=rotation,
        scale=scale,
        translate_x=translate_x,
        translate_y=translate_y,
        warnings=[],
    )


def _resize(image: Any, *, scale: float, cv2: Any) -> Any:
    if _is_identity_float(scale):
        return image
    width = max(1, round(image.shape[1] * scale))
    height = max(1, round(image.shape[0] * scale))
    return cv2.resize(image, (width, height), interpolation=cv2.INTER_AREA)


def _extract_reference_patch(
    template: Any, *, match_scale: float, cv2: Any
) -> tuple[Any, int, int]:
    scaled = _resize(template, scale=match_scale, cv2=cv2)
    blurred = cv2.GaussianBlur(scaled, (0, 0), 1.5)
    height, width = blurred.shape
    top = 0
    bottom = max(1, round(height * _TOP_FRACTION))
    left = round(width * _CENTER_TRIM_FRACTION)
    right = width - left
    if right <= left:
        left = 0
        right = width
    return blurred[top:bottom, left:right], left, top


def _warp_student(image: Any, *, angle_deg: float, scale: float, cv2: Any) -> Any:
    height, width = image.shape
    center = (width / 2.0, height / 2.0)
    matrix = cv2.getRotationMatrix2D(center, angle_deg, scale)
    cos = abs(matrix[0, 0])
    sin = abs(matrix[0, 1])
    bound_width = max(1, int((height * sin) + (width * cos)))
    bound_height = max(1, int((height * cos) + (width * sin)))
    matrix[0, 2] += (bound_width / 2.0) - center[0]
    matrix[1, 2] += (bound_height / 2.0) - center[1]
    return cv2.warpAffine(
        image,
        matrix,
        (bound_width, bound_height),
        flags=cv2.INTER_LINEAR,
        borderMode=cv2.BORDER_CONSTANT,
        borderValue=255,
    )


def _candidate_match(
    *,
    student: Any,
    patch: Any,
    patch_left: int,
    patch_top: int,
    scale: float,
    angle_deg: float,
    cv2: Any,
) -> tuple[float, float, float]:
    transformed = _warp_student(student, angle_deg=angle_deg, scale=scale, cv2=cv2)
    transformed = cv2.GaussianBlur(transformed, (0, 0), 1.5)
    if transformed.shape[0] < patch.shape[0] or transformed.shape[1] < patch.shape[1]:
        return -1.0, 0.0, 0.0
    max_left = transformed.shape[1] - patch.shape[1]
    max_top = transformed.shape[0] - patch.shape[0]
    margin_x = max(16, round(transformed.shape[1] * _SEARCH_MARGIN_FRACTION))
    margin_y = max(16, round(transformed.shape[0] * _SEARCH_MARGIN_FRACTION))
    search_left = max(0, patch_left - margin_x)
    search_top = max(0, patch_top - margin_y)
    search_right = min(max_left, patch_left + margin_x)
    search_bottom = min(max_top, patch_top + margin_y)
    region = transformed[
        search_top : search_bottom + patch.shape[0],
        search_left : search_right + patch.shape[1],
    ]
    match = cv2.matchTemplate(region, patch, cv2.TM_CCOEFF_NORMED)
    _min_val, max_val, _min_loc, max_loc = cv2.minMaxLoc(match)
    matched_left = search_left + max_loc[0]
    matched_top = search_top + max_loc[1]
    translate_x = float(patch_left - matched_left)
    translate_y = float(patch_top - matched_top)
    return float(max_val), translate_x, translate_y


def _fine_candidates(best_scale: float, best_angle: float) -> list[tuple[float, float]]:
    candidates: list[tuple[float, float]] = []
    for scale_delta in _FINE_SCALE_STEPS:
        scale = max(0.5, best_scale + scale_delta)
        for angle_delta in _FINE_ANGLE_STEPS:
            candidates.append((scale, best_angle + angle_delta))
    return candidates


def _full_res_candidates(best_scale: float, best_angle: float) -> list[tuple[float, float]]:
    return [(best_scale, best_angle + angle_delta) for angle_delta in _FULL_RES_ANGLE_STEPS]


def _classify_status(score: float) -> Literal["ok", "low_confidence", "failed"]:
    if score >= _STATUS_OK_MIN:
        return "ok"
    if score >= _STATUS_LOW_CONFIDENCE_MIN:
        return "low_confidence"
    return "failed"


def _score_at_expected_location(
    *, image: Any, patch: Any, patch_left: int, patch_top: int, cv2: Any
) -> float:
    max_left = image.shape[1] - patch.shape[1]
    max_top = image.shape[0] - patch.shape[0]
    if patch_left < 0 or patch_top < 0 or patch_left > max_left or patch_top > max_top:
        return -1.0
    region = image[patch_top : patch_top + patch.shape[0], patch_left : patch_left + patch.shape[1]]
    if region.shape != patch.shape:
        return -1.0
    match = cv2.matchTemplate(region, patch, cv2.TM_CCOEFF_NORMED)
    return float(match[0, 0])


def _candidate_rank(
    *,
    score: float,
    angle_deg: float,
    scale: float,
    translate_x: float,
    translate_y: float,
) -> tuple[float, float, float, float]:
    adjusted_score = score - (abs(angle_deg) * 0.03) - (abs(scale - 1.0) * 2.0)
    adjusted_score -= (abs(translate_x) + abs(translate_y)) * _TRANSLATION_PENALTY
    return (
        adjusted_score,
        round(score, 6),
        -(abs(translate_x) + abs(translate_y)),
        -abs(angle_deg),
    )


@dataclass(frozen=True)
class _MatchState:
    score: float
    scale: float
    angle: float
    translate_x: float
    translate_y: float
    translation_scale: float
    rank: tuple[float, float, float, float]
    identity_score: float


def _match_state(
    *,
    score: float,
    scale: float,
    angle: float,
    translate_x: float,
    translate_y: float,
    translation_scale: float,
    identity_score: float,
) -> _MatchState:
    return _MatchState(
        score=score,
        scale=scale,
        angle=angle,
        translate_x=translate_x,
        translate_y=translate_y,
        translation_scale=translation_scale,
        rank=_candidate_rank(
            score=score,
            angle_deg=angle,
            scale=scale,
            translate_x=translate_x,
            translate_y=translate_y,
        ),
        identity_score=identity_score,
    )


def _initial_match_state(
    *,
    student_scaled: Any,
    patch: Any,
    patch_left: int,
    patch_top: int,
    match_scale: float,
    cv2: Any,
) -> _MatchState:
    identity_score = _score_at_expected_location(
        image=student_scaled,
        patch=patch,
        patch_left=patch_left,
        patch_top=patch_top,
        cv2=cv2,
    )
    return _match_state(
        score=identity_score,
        scale=1.0,
        angle=0.0,
        translate_x=0.0,
        translate_y=0.0,
        translation_scale=match_scale,
        identity_score=identity_score,
    )


def _maybe_better_match(
    state: _MatchState,
    *,
    score: float,
    scale: float,
    angle: float,
    translate_x: float,
    translate_y: float,
    translation_scale: float,
) -> _MatchState:
    rank = _candidate_rank(
        score=score,
        angle_deg=angle,
        scale=scale,
        translate_x=translate_x,
        translate_y=translate_y,
    )
    if rank <= state.rank:
        return state
    return _MatchState(
        score=score,
        scale=scale,
        angle=angle,
        translate_x=translate_x,
        translate_y=translate_y,
        translation_scale=translation_scale,
        rank=rank,
        identity_score=state.identity_score,
    )


def _search_candidates(
    state: _MatchState,
    *,
    student: Any,
    patch: Any,
    patch_left: int,
    patch_top: int,
    candidates: list[tuple[float, float]],
    translation_scale: float,
    cv2: Any,
    early_stop: float | None = None,
) -> _MatchState:
    current = state
    for scale, angle in candidates:
        score, translate_x, translate_y = _candidate_match(
            student=student,
            patch=patch,
            patch_left=patch_left,
            patch_top=patch_top,
            scale=scale,
            angle_deg=angle,
            cv2=cv2,
        )
        current = _maybe_better_match(
            current,
            score=score,
            scale=scale,
            angle=angle,
            translate_x=translate_x,
            translate_y=translate_y,
            translation_scale=translation_scale,
        )
        if early_stop is not None and current.score >= early_stop:
            break
    return current


def _refine_full_resolution_match(
    state: _MatchState,
    *,
    template: Any,
    student_full: Any,
    cv2: Any,
) -> _MatchState:
    full_patch, full_left, full_top = _extract_reference_patch(template, match_scale=1.0, cv2=cv2)
    full_identity_score = _score_at_expected_location(
        image=student_full,
        patch=full_patch,
        patch_left=full_left,
        patch_top=full_top,
        cv2=cv2,
    )
    refined = state
    if full_identity_score > refined.identity_score:
        refined = _MatchState(
            score=refined.score,
            scale=refined.scale,
            angle=refined.angle,
            translate_x=refined.translate_x,
            translate_y=refined.translate_y,
            translation_scale=refined.translation_scale,
            rank=refined.rank,
            identity_score=full_identity_score,
        )
    refined = _maybe_better_match(
        refined,
        score=full_identity_score,
        scale=1.0,
        angle=0.0,
        translate_x=0.0,
        translate_y=0.0,
        translation_scale=1.0,
    )
    return _search_candidates(
        refined,
        student=student_full,
        patch=full_patch,
        patch_left=full_left,
        patch_top=full_top,
        candidates=_full_res_candidates(refined.scale, refined.angle),
        translation_scale=1.0,
        cv2=cv2,
    )


def _normalize_translation(state: _MatchState) -> _MatchState:
    if _is_identity_float(state.translation_scale):
        return state
    return _MatchState(
        score=state.score,
        scale=state.scale,
        angle=state.angle,
        translate_x=state.translate_x / state.translation_scale,
        translate_y=state.translate_y / state.translation_scale,
        translation_scale=1.0,
        rank=state.rank,
        identity_score=state.identity_score,
    )


def _snap_to_identity(state: _MatchState) -> _MatchState:
    if not (
        state.identity_score >= state.score - _IDENTITY_SCORE_EPSILON
        and abs(state.angle) <= _IDENTITY_SNAP_MAX_ROTATION
        and abs(state.scale - 1.0) <= _IDENTITY_SNAP_MAX_SCALE_DELTA
        and abs(state.translate_x) <= _IDENTITY_SNAP_MAX_TRANSLATION
        and abs(state.translate_y) <= _IDENTITY_SNAP_MAX_TRANSLATION
    ):
        return state
    return _match_state(
        score=state.identity_score,
        scale=1.0,
        angle=0.0,
        translate_x=0.0,
        translate_y=0.0,
        translation_scale=1.0,
        identity_score=state.identity_score,
    )


def _template_match_alignment(
    *,
    template: Any,
    student: Any,
    mode: Literal["fast", "precise"],
    include_marker_fallback_warning: bool,
    cv2: Any,
) -> AlignmentResponse:
    match_scale = 1.0 if mode == "precise" else _FAST_MATCH_SCALE
    patch, patch_left, patch_top = _extract_reference_patch(
        template, match_scale=match_scale, cv2=cv2
    )
    student_scaled = cv2.GaussianBlur(_resize(student, scale=match_scale, cv2=cv2), (0, 0), 1.5)
    student_full = cv2.GaussianBlur(student, (0, 0), 1.5)

    state = _initial_match_state(
        student_scaled=student_scaled,
        patch=patch,
        patch_left=patch_left,
        patch_top=patch_top,
        match_scale=match_scale,
        cv2=cv2,
    )
    state = _search_candidates(
        state,
        student=student_scaled,
        patch=patch,
        patch_left=patch_left,
        patch_top=patch_top,
        candidates=[(scale, angle) for scale in _COARSE_SCALES for angle in _COARSE_ANGLES],
        translation_scale=match_scale,
        cv2=cv2,
        early_stop=_COARSE_EARLY_STOP,
    )
    state = _search_candidates(
        state,
        student=student_scaled,
        patch=patch,
        patch_left=patch_left,
        patch_top=patch_top,
        candidates=_fine_candidates(state.scale, state.angle),
        translation_scale=match_scale,
        cv2=cv2,
    )
    if not _is_identity_float(match_scale):
        state = _refine_full_resolution_match(
            state, template=template, student_full=student_full, cv2=cv2
        )

    state = _snap_to_identity(_normalize_translation(state))
    warnings = [_marker_fallback_warning()] if include_marker_fallback_warning else []
    status = _classify_status(state.score)
    if status == "failed":
        return AlignmentResponse(status="failed", confidence=state.score, warnings=warnings)
    return AlignmentResponse(
        status=status,
        confidence=state.score,
        rotation=-state.angle,
        scale=1.0 / state.scale,
        translate_x=state.translate_x,
        translate_y=state.translate_y,
        warnings=warnings,
    )


@dataclass(frozen=True)
class CoreTemplateMatchProvider(AlignmentProvider):
    """Built-in alignment provider backed by OpenCV template matching."""

    provider_name: str = "core_template_match"
    interface_version: str = PROVIDER_INTERFACE_VERSION

    @property
    def capability(self) -> Literal["alignment_engine"]:
        return "alignment_engine"

    def align(self, request: AlignmentRequest) -> AlignmentResponse:
        cv2, np = _load_cv_dependencies()
        template = _read_grayscale(request.template_page_path, cv2=cv2)
        student = _read_grayscale(request.student_page_path, cv2=cv2)
        if request.marker_mode == "prefer_aruco":
            aruco_response = _estimate_aruco_alignment(
                template=template, student=student, cv2=cv2, np=np
            )
            if aruco_response is not None:
                return aruco_response
        return _template_match_alignment(
            template=template,
            student=student,
            mode=request.mode,
            include_marker_fallback_warning=request.marker_mode == "prefer_aruco",
            cv2=cv2,
        )

# SPDX-License-Identifier: AGPL-3.0-only
"""Student-specific PII candidate matching for scans.pii."""

from __future__ import annotations

import re
from collections.abc import Iterable
from dataclasses import dataclass

from scriptscore.pii_scan.images import region_ink_fraction
from scriptscore.pii_scan.types import RasterBundle, SensitiveHit, VisionToken

EMAIL_PATTERN = re.compile(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b", re.IGNORECASE)
PHONE_PATTERN = re.compile(r"(?:(?:\+?\d[\d .()/-]{7,}\d))")
HANDLE_PATTERN = re.compile(r"\B@[A-Za-z][A-Za-z0-9_.-]{2,31}\b")
NAME_BLOCKLIST = {
    "question",
    "score",
    "maximum",
    "primitive",
    "reference",
    "types",
    "java",
    "boolean",
    "string",
    "float",
}


@dataclass(frozen=True, slots=True)
class TriggerLexicon:
    """Normalized trigger lookup derived from the caller-supplied roster values."""

    emails: frozenset[str]
    phones: frozenset[str]
    usernames: frozenset[str]
    names: frozenset[str]
    name_parts: frozenset[str]
    has_name_trigger: bool


@dataclass(frozen=True, slots=True)
class _ScanContext:
    text: str
    tokens: list[VisionToken]
    raster: RasterBundle


def _compact_text(value: str) -> str:
    return re.sub(r"\s+", " ", value).strip()


def _letters_only(value: str) -> str:
    return re.sub(r"[^a-z]", "", value.lower())


def _normalized_name(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z' -]", " ", value)
    return _compact_text(cleaned).lower()


def _normalized_phone(value: str) -> str:
    replacements = str.maketrans({"O": "0", "o": "0", "I": "1", "l": "1", "S": "5"})
    return re.sub(r"\D", "", value.translate(replacements))


def _normalized_username(value: str) -> str:
    lowered = value.strip().lower()
    return lowered[1:] if lowered.startswith("@") else lowered


def _bounded_edit_distance(left: str, right: str, *, max_distance: int) -> int | None:
    """Return the edit distance when it stays within the supplied bound."""

    if left == right:
        return 0
    if abs(len(left) - len(right)) > max_distance:
        return None
    previous = list(range(len(right) + 1))
    for left_index, left_char in enumerate(left, start=1):
        current = [left_index]
        row_min = current[0]
        for right_index, right_char in enumerate(right, start=1):
            substitution_cost = 0 if left_char == right_char else 1
            value = min(
                previous[right_index] + 1,
                current[right_index - 1] + 1,
                previous[right_index - 1] + substitution_cost,
            )
            current.append(value)
            row_min = min(row_min, value)
        if row_min > max_distance:
            return None
        previous = current
    distance = previous[-1]
    return distance if distance <= max_distance else None


def _fuzzy_email_matches_trigger(candidate: str, lexicon: TriggerLexicon) -> bool:
    """Return whether one OCR email candidate is a conservative near-match."""

    normalized_candidate = candidate.lower()
    if normalized_candidate in lexicon.emails:
        return True
    if normalized_candidate.count("@") != 1:
        return False
    candidate_local, candidate_domain = normalized_candidate.split("@", 1)
    for trigger in lexicon.emails:
        if trigger.count("@") != 1:
            continue
        trigger_local, trigger_domain = trigger.split("@", 1)
        if candidate_local != trigger_local:
            continue
        if _bounded_edit_distance(candidate_domain, trigger_domain, max_distance=1) is not None:
            return True
    return False


def _fuzzy_name_part_matches(candidate: str, trigger_part: str) -> bool:
    """Return whether one OCR word is a conservative near-match for one name part."""

    normalized_candidate = _letters_only(candidate)
    normalized_trigger = _letters_only(trigger_part)
    if not normalized_candidate or not normalized_trigger:
        return False
    if normalized_candidate == normalized_trigger:
        return True
    if normalized_candidate[0] != normalized_trigger[0]:
        return False
    shortest = min(len(normalized_candidate), len(normalized_trigger))
    longest = max(len(normalized_candidate), len(normalized_trigger))
    if shortest < 5 or longest - shortest > 1:
        return False
    return (
        _bounded_edit_distance(
            normalized_candidate,
            normalized_trigger,
            max_distance=1,
        )
        is not None
    )


def build_trigger_lexicon(trigger_words: list[str]) -> TriggerLexicon:
    """Classify trigger phrases into normalized lookup tables."""

    emails: set[str] = set()
    phones: set[str] = set()
    usernames: set[str] = set()
    names: set[str] = set()
    name_parts: set[str] = set()

    for trigger in trigger_words:
        trimmed = trigger.strip()
        if not trimmed:
            continue
        if EMAIL_PATTERN.fullmatch(trimmed):
            emails.add(trimmed.lower())
            usernames.add(_normalized_username(trimmed.split("@", 1)[0]))
            continue
        digits = _normalized_phone(trimmed)
        if len(digits) >= 10:
            phones.add(digits)
        username_form = _normalized_username(trimmed)
        if username_form and " " not in trimmed and len(username_form) >= 3:
            usernames.add(username_form)
        normalized_name = _normalized_name(trimmed)
        if normalized_name and any(part.isalpha() for part in trimmed):
            names.add(normalized_name)
            name_parts.update(
                part
                for part in normalized_name.split()
                if len(_letters_only(part)) >= 4 and part not in NAME_BLOCKLIST
            )

    return TriggerLexicon(
        emails=frozenset(emails),
        phones=frozenset(phones),
        usernames=frozenset(usernames),
        names=frozenset(names),
        name_parts=frozenset(name_parts),
        has_name_trigger=bool(names),
    )


def detect_student_pii(
    *,
    extracted_text: str,
    tokens: list[VisionToken],
    raster: RasterBundle,
    trigger_words: list[str],
) -> list[SensitiveHit]:
    """Return student-specific PII hits based on OCR candidates and trigger values."""

    context = _ScanContext(
        text=_compact_text(extracted_text),
        tokens=tokens,
        raster=raster,
    )
    lexicon = build_trigger_lexicon(trigger_words)
    hits: list[SensitiveHit] = []
    hits.extend(_text_pattern_hits(context, lexicon))
    hits.extend(_labeled_hits(context, lexicon))
    hits.extend(_sentence_name_hits(context, lexicon))
    hits.extend(_standalone_name_hits(context, lexicon))
    return _dedupe_hits(hits)


def _text_pattern_hits(context: _ScanContext, lexicon: TriggerLexicon) -> list[SensitiveHit]:
    hits: list[SensitiveHit] = []
    seen: set[tuple[str, str]] = set()

    for match in EMAIL_PATTERN.finditer(context.text):
        snippet = match.group(0)
        key = ("email", snippet.lower())
        if _fuzzy_email_matches_trigger(snippet, lexicon) and key not in seen:
            hits.append(
                SensitiveHit(
                    kind="email",
                    snippet=snippet,
                    confidence=0.99 if snippet.lower() in lexicon.emails else 0.94,
                    reason=(
                        "OCR text matches a provided student email trigger"
                        if snippet.lower() in lexicon.emails
                        else "OCR text is a one-edit near-match for a provided student email trigger"
                    ),
                )
            )
            seen.add(key)

    for match in PHONE_PATTERN.finditer(context.text):
        snippet = match.group(0).strip()
        normalized = _normalized_phone(snippet)
        key = ("phone_number", normalized)
        if len(normalized) >= 10 and normalized in lexicon.phones and key not in seen:
            hits.append(
                SensitiveHit(
                    kind="phone_number",
                    snippet=snippet,
                    confidence=0.96,
                    reason="OCR text matches a provided student phone trigger",
                )
            )
            seen.add(key)

    for match in HANDLE_PATTERN.finditer(context.text):
        snippet = match.group(0)
        normalized = _normalized_username(snippet)
        key = ("username", normalized)
        if normalized in lexicon.usernames and key not in seen:
            hits.append(
                SensitiveHit(
                    kind="username",
                    snippet=snippet,
                    confidence=0.94,
                    reason="OCR text matches a provided student username trigger",
                )
            )
            seen.add(key)

    return hits


def _group_token_lines(tokens: list[VisionToken]) -> list[list[VisionToken]]:
    if not tokens:
        return []
    ordered = sorted(tokens, key=lambda token: (token.center_y, token.left))
    lines: list[list[VisionToken]] = []
    for token in ordered:
        if not lines:
            lines.append([token])
            continue
        current_line = lines[-1]
        baseline = sum(item.center_y for item in current_line) / len(current_line)
        average_height = sum(item.height for item in current_line) / len(current_line)
        if abs(token.center_y - baseline) <= max(12.0, average_height * 0.6):
            current_line.append(token)
            continue
        lines.append([token])
    for line in lines:
        line.sort(key=lambda token: token.left)
    return lines


def _username_label_window(line: list[VisionToken]) -> tuple[int, int] | None:
    normalized = [_letters_only(token.text) for token in line]
    for index, token in enumerate(normalized[:3]):
        if token in {"userid", "username", "login"}:
            return index, index
        if token == "user" and index + 1 < len(normalized) and normalized[index + 1] == "id":
            return index, index + 1
        if token == "handle" and ":" in line[index].text:
            return index, index
    return None


def _name_matches_trigger(snippet: str, lexicon: TriggerLexicon) -> bool:
    normalized = _normalized_name(snippet)
    if not normalized:
        return False
    candidate_parts = normalized.split()
    if normalized in lexicon.names:
        return True
    for trigger in lexicon.names:
        trigger_parts = trigger.split()
        if normalized in trigger or trigger in normalized:
            return True
        if candidate_parts and all(part in trigger_parts for part in candidate_parts):
            return True
    return False


def _sentence_name_hits(context: _ScanContext, lexicon: TriggerLexicon) -> list[SensitiveHit]:
    """Detect a student name when multiple provided name parts appear in one OCR line."""

    if len(lexicon.name_parts) < 2:
        return []
    hits: list[SensitiveHit] = []
    for line in _group_token_lines(context.tokens):
        matched_parts: dict[str, str] = {}
        for token in line:
            for word in re.findall(r"[A-Za-z']+", token.text):
                stripped = word.lstrip("'\"([{")
                if not stripped or not stripped[0].isupper():
                    continue
                for trigger_part in lexicon.name_parts:
                    if _fuzzy_name_part_matches(word, trigger_part):
                        matched_parts.setdefault(trigger_part, word)
        if len(matched_parts) < 2:
            continue
        snippet_parts: list[str] = []
        for token in line:
            matched_words = [
                word
                for word in re.findall(r"[A-Za-z']+", token.text)
                if any(
                    _fuzzy_name_part_matches(word, trigger_part)
                    for trigger_part in lexicon.name_parts
                )
            ]
            snippet_parts.extend(matched_words)
        hits.append(
            SensitiveHit(
                kind="name",
                snippet=" ".join(snippet_parts),
                confidence=0.86,
                reason="multiple provided student-name parts appear in one handwritten OCR line",
            )
        )
    return _dedupe_hits(hits)


def _labeled_hits(context: _ScanContext, lexicon: TriggerLexicon) -> list[SensitiveHit]:
    hits: list[SensitiveHit] = []
    lines = _group_token_lines(context.tokens)
    width, height = context.raster.working_size
    normalized_words = {_letters_only(token.text) for token in context.tokens}
    form_header = "score" in normalized_words and (
        any(word.endswith("uestion") or "question" in word for word in normalized_words)
        or "maximum" in normalized_words
    )

    for line in lines:
        line_text = " ".join(token.text for token in line)
        compact_line = _compact_text(line_text)
        lower_line = compact_line.lower()

        if "email" in lower_line:
            for match in EMAIL_PATTERN.finditer(compact_line):
                snippet = match.group(0)
                if snippet.lower() in lexicon.emails:
                    hits.append(
                        SensitiveHit(
                            kind="email",
                            snippet=snippet,
                            confidence=0.98,
                            reason="email label line contains a provided student email",
                        )
                    )

        username_window = _username_label_window(line)
        if username_window is not None:
            _, end_index = username_window
            trailing = [token.text for index, token in enumerate(line) if index > end_index]
            candidate = _compact_text(" ".join(trailing))
            if candidate and _normalized_username(candidate) in lexicon.usernames:
                hits.append(
                    SensitiveHit(
                        kind="username",
                        snippet=candidate,
                        confidence=0.9,
                        reason="username label is followed by a provided student identifier",
                    )
                )

        if any(label in _letters_only(compact_line) for label in ("phone", "telephone", "mobile")):
            phone_match = PHONE_PATTERN.search(compact_line)
            if phone_match is not None:
                snippet = phone_match.group(0).strip()
                if _normalized_phone(snippet) in lexicon.phones:
                    hits.append(
                        SensitiveHit(
                            kind="phone_number",
                            snippet=snippet,
                            confidence=0.92,
                            reason="phone label line contains a provided student phone number",
                        )
                    )

        for index, token in enumerate(line):
            if _letters_only(token.text) not in {"name", "names"}:
                continue

            ink_fraction = region_ink_fraction(
                context.raster.binary_mask,
                left=token.right + 4,
                top=max(0, token.top - int(token.height * 1.2)),
                right=min(width, max(token.right + 16, token.right + int(width * 0.6))),
                bottom=min(height, token.bottom + int(token.height * 1.2)),
            )
            trailing_words = [
                candidate.text
                for candidate in line[index + 1 :]
                if any(character.isalpha() for character in candidate.text)
            ]
            if trailing_words:
                snippet = " ".join(trailing_words[:4])
                if _name_matches_trigger(snippet, lexicon):
                    hits.append(
                        SensitiveHit(
                            kind="name",
                            snippet=snippet,
                            confidence=0.88,
                            reason="name label is followed by text matching a provided student name",
                        )
                    )
                continue

            if ink_fraction > 0.015 and lexicon.has_name_trigger:
                hits.append(
                    SensitiveHit(
                        kind="name",
                        snippet="",
                        confidence=0.76,
                        reason="name label is followed by handwritten ink and a student name trigger exists",
                    )
                )

        if form_header and ":" in compact_line:
            label_fragment = compact_line.split(":", 1)[0].strip().lower()
            if label_fragment in {"e", "me", "ame"} and lexicon.has_name_trigger:
                first = line[0]
                ink_fraction = region_ink_fraction(
                    context.raster.binary_mask,
                    left=first.right + 4,
                    top=max(0, first.top - int(first.height * 1.2)),
                    right=min(width, max(first.right + 16, first.right + int(width * 0.7))),
                    bottom=min(height, first.bottom + int(first.height * 1.6)),
                )
                if ink_fraction > 0.02:
                    hits.append(
                        SensitiveHit(
                            kind="name",
                            snippet="",
                            confidence=0.7,
                            reason="form-style name label is followed by handwriting",
                        )
                    )

    if form_header and lexicon.has_name_trigger:
        header_tokens = [
            re.sub(r"[^A-Za-z'-]", "", token.text)
            for token in context.tokens
            if token.center_y <= context.raster.working_size[1] * 0.45
            and any(character.isalpha() for character in token.text)
        ]
        candidates: list[str] = []
        for header_token in header_tokens:
            cleaned = header_token.strip()
            if len(cleaned) < 4:
                continue
            lowered = cleaned.lower()
            if lowered in NAME_BLOCKLIST:
                continue
            if not cleaned[0].isupper():
                continue
            candidates.append(cleaned)
        if candidates:
            snippet = " ".join(candidates[:2])
            if _name_matches_trigger(snippet, lexicon):
                hits.append(
                    SensitiveHit(
                        kind="name",
                        snippet=snippet,
                        confidence=0.72,
                        reason="top-of-page handwritten name candidate matches the provided student name",
                    )
                )

    return _dedupe_hits(hits)


def _standalone_name_hits(context: _ScanContext, lexicon: TriggerLexicon) -> list[SensitiveHit]:
    if not lexicon.has_name_trigger:
        return []
    hits: list[SensitiveHit] = []
    for line in _group_token_lines(context.tokens):
        tokens = [re.sub(r"[^A-Za-z'-]", "", token.text) for token in line]
        clean_tokens = [token for token in tokens if token]
        if not 2 <= len(clean_tokens) <= 3:
            continue
        if any(token.lower() in NAME_BLOCKLIST for token in clean_tokens):
            continue
        if any(len(token) > 14 for token in clean_tokens):
            continue
        if sum(len(token) for token in clean_tokens) > 28:
            continue
        if not all(token[0].isupper() for token in clean_tokens):
            continue
        line_span = max(token.right for token in line) - min(token.left for token in line)
        if line_span > context.raster.working_size[0] * 0.5:
            continue
        snippet = " ".join(clean_tokens)
        if _name_matches_trigger(snippet, lexicon):
            hits.append(
                SensitiveHit(
                    kind="name",
                    snippet=snippet,
                    confidence=0.55,
                    reason="capitalized token sequence matches the provided student name",
                )
            )
    return _dedupe_hits(hits)


def _dedupe_hits(hits: Iterable[SensitiveHit]) -> list[SensitiveHit]:
    deduped: list[SensitiveHit] = []
    seen: set[tuple[str, str]] = set()
    for hit in hits:
        snippet_key = hit.snippet.strip().lower() if hit.snippet else "<none>"
        marker = (hit.kind, snippet_key)
        if marker in seen:
            continue
        deduped.append(hit)
        seen.add(marker)
    deduped.sort(key=lambda item: (-item.confidence, item.kind, item.snippet.lower()))
    return deduped

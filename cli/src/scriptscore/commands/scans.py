# SPDX-License-Identifier: AGPL-3.0-only
"""Compatibility exports for scan command specs."""

from scriptscore.commands.scans_canonicalize import (
    handle_scans_canonicalize,
    scans_canonicalize_spec,
)
from scriptscore.commands.scans_crop import handle_scans_crop, scans_crop_spec
from scriptscore.commands.scans_ingest import handle_scans_ingest, scans_ingest_spec
from scriptscore.commands.scans_parse import handle_scans_parse, scans_parse_spec
from scriptscore.commands.scans_pii import handle_scans_pii, scans_pii_spec
from scriptscore.commands.scans_transform import handle_scans_transform, scans_transform_spec

__all__ = [
    "handle_scans_canonicalize",
    "handle_scans_crop",
    "handle_scans_ingest",
    "handle_scans_parse",
    "handle_scans_pii",
    "handle_scans_transform",
    "scans_canonicalize_spec",
    "scans_crop_spec",
    "scans_ingest_spec",
    "scans_parse_spec",
    "scans_pii_spec",
    "scans_transform_spec",
]

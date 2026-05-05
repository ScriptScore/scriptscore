// SPDX-License-Identifier: AGPL-3.0-only
import type {
  StudentIntakeInput,
  StudentIntakeRasterSize,
  StudentIntakeRedactionRegionInput
} from '$lib/types';

function roundPxRegion(region: StudentIntakeRedactionRegionInput): StudentIntakeRedactionRegionInput {
  return {
    pageNumber: Math.round(region.pageNumber),
    x: Math.round(region.x),
    y: Math.round(region.y),
    width: Math.round(region.width),
    height: Math.round(region.height)
  };
}

function roundRasterSize(size: StudentIntakeRasterSize): StudentIntakeRasterSize {
  return {
    widthPx: Math.round(size.widthPx),
    heightPx: Math.round(size.heightPx)
  };
}

/**
 * Coerce pixel integers for Tauri/serde: the UI uses float math (scaling, drag) but the host expects JSON integers (i64).
 */
export function sanitizeStudentIntakeInputForHost(input: StudentIntakeInput): StudentIntakeInput {
  const desiredPageOrder = (input.desiredPageOrder ?? []).map((n) => Math.round(n));
  const redactionRegionsPx = (input.redactionRegionsPx ?? []).map(roundPxRegion);
  const rasterSizesByPage: Record<number, StudentIntakeRasterSize> = {};
  for (const [key, size] of Object.entries(input.rasterSizesByPage ?? {})) {
    rasterSizesByPage[Math.round(Number(key))] = roundRasterSize(size);
  }
  const localStudentName = input.localStudentName?.trim() || null;
  return {
    studentRef: input.studentRef,
    ...(localStudentName ? { localStudentName } : {}),
    rawPdfPath: input.rawPdfPath,
    desiredPageOrder,
    redactionRegionsPx,
    rasterSizesByPage
  };
}

export function sanitizeStudentIntakeInputsForHost(inputs: StudentIntakeInput[]): StudentIntakeInput[] {
  return inputs.map(sanitizeStudentIntakeInputForHost);
}

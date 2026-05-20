// SPDX-License-Identifier: AGPL-3.0-only
export function normalizeDesiredPageOrder(
  availablePageNumbers: number[],
  desiredPageOrder?: number[] | null
): number[] {
  if (!Array.isArray(desiredPageOrder) || desiredPageOrder.length === 0) {
    return [...availablePageNumbers];
  }

  const available = new Set(availablePageNumbers);
  const seen = new Set<number>();
  const desired: number[] = [];
  for (const pageNumber of desiredPageOrder) {
    if (!Number.isInteger(pageNumber) || pageNumber <= 0 || !available.has(pageNumber)) {
      continue;
    }
    if (seen.has(pageNumber)) {
      continue;
    }
    seen.add(pageNumber);
    desired.push(pageNumber);
  }
  return desired.length > 0 ? desired : [...availablePageNumbers];
}

export function reorderPageNumbers(
  currentOrder: number[],
  draggedPageNumber: number,
  targetPageNumber: number
): number[] {
  if (draggedPageNumber === targetPageNumber) {
    return [...currentOrder];
  }
  const draggedIndex = currentOrder.indexOf(draggedPageNumber);
  const targetIndex = currentOrder.indexOf(targetPageNumber);
  if (draggedIndex < 0 || targetIndex < 0) {
    return [...currentOrder];
  }
  const nextOrder = [...currentOrder];
  nextOrder.splice(draggedIndex, 1);
  nextOrder.splice(targetIndex, 0, draggedPageNumber);
  return nextOrder;
}

// SPDX-License-Identifier: AGPL-3.0-only
export function normalizeDesiredPageOrder(
  availablePageNumbers: number[],
  desiredPageOrder?: number[] | null
): number[] {
  if (!Array.isArray(desiredPageOrder) || desiredPageOrder.length === 0) {
    return [...availablePageNumbers];
  }

  const available = [...availablePageNumbers].sort((left, right) => left - right);
  const desired = desiredPageOrder.filter((pageNumber) => Number.isInteger(pageNumber) && pageNumber > 0);
  const sortedDesired = [...desired].sort((left, right) => left - right);
  if (available.length !== desired.length) {
    return [...availablePageNumbers];
  }
  for (let index = 0; index < available.length; index += 1) {
    if (available[index] !== sortedDesired[index]) {
      return [...availablePageNumbers];
    }
  }
  return [...desired];
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

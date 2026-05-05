<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import ConfirmDialog from './ConfirmDialog.svelte';
  import InteractiveCanvasSurface from './InteractiveCanvasSurface.svelte';
  import SelectionHandle from './SelectionHandle.svelte';
  import {
    defaultImageRegionPresentation,
    type ImageRegion,
    type ImageRegionMetrics,
    type ImageRegionPresentationResolver
  } from './imageRegionEditor';
  import { feedbackToneClass } from './feedback';
  import {
    clamp,
    computeRegionStyleMap,
    draftImageRegion,
    hitTestImageRegions,
    regionStyleKey,
    resizeImageRegion,
    type ImageRegionPoint,
    type ImageRegionRect,
    type ImageRegionResizeHandle
  } from './imageRegionGeometry';

  type DragState =
    | { kind: 'draw'; start: ImageRegionPoint; current: ImageRegionPoint }
    | { kind: 'move'; regionId: string; start: ImageRegionPoint; origin: ImageRegionRect }
    | {
        kind: 'resize';
        regionId: string;
        start: ImageRegionPoint;
        origin: ImageRegionRect;
        handle: ImageRegionResizeHandle;
      }
    | null;

  export let imageSrc = '';
  export let imageAlt = 'Image preview';
  export let pageNumber = 1;
  export let imageWidth = 0;
  export let imageHeight = 0;
  export let regions: ImageRegion[] = [];
  export let disabled = false;
  export let ariaLabel = 'Image region editor';
  export let squareCorners = false;
  export let regionPresentation: ImageRegionPresentationResolver = defaultImageRegionPresentation;
  export let maxRegions: number | null = null;
  export let deleteTitle = 'Delete region?';
  export let deleteDescription = 'Remove this region from the page.';
  export let deleteConfirmLabel = 'Delete';
  export let onRegionsChange: ((regions: ImageRegion[]) => void | Promise<void>) | null = null;
  export let onSelectionChange: ((index: number | null) => void) | null = null;
  export let onMetricsChange: ((metrics: ImageRegionMetrics) => void) | null = null;
  export let containerMinHeightClass = 'min-h-[28rem]';
  export let constrainHeight = true;

  let imageElement: HTMLImageElement | null = null;
  let frameElement: HTMLDivElement | null = null;
  let naturalWidth = 0;
  let naturalHeight = 0;
  let frameWidth = 0;
  let frameHeight = 0;
  let localRegions: ImageRegion[] = [];
  let draftPreviewRegion: ImageRegion | null = null;
  let dragState: DragState = null;
  let syncKey = '';
  let imageMetricsKey = '';
  let imageLoadError = false;
  let activePointerId: number | null = null;
  let imageLoadToken = 0;
  let resizeTick = 0;
  let pendingDeleteRegionId: string | null = null;
  let activeRegionId: string | null = null;

  $: visibleRegions = [...localRegions, ...(draftPreviewRegion ? [draftPreviewRegion] : [])];
  $: {
    const nextMetricsKey = `${imageSrc}|${imageWidth}|${imageHeight}`;
    if (nextMetricsKey !== imageMetricsKey) {
      imageMetricsKey = nextMetricsKey;
      void syncImageMetadata();
    }
  }
  $: {
    const nextKey = JSON.stringify(
      regions.map((region) => [
        region.regionId,
        region.pageNumber,
        region.x,
        region.y,
        region.width,
        region.height,
        region.kind
      ])
    );
    if (!dragState && nextKey !== syncKey) {
      localRegions = regions.map((region) => ({ ...region }));
      syncKey = nextKey;
      if (activeRegionId && !localRegions.some((region) => region.regionId === activeRegionId)) {
        activeRegionId = null;
      }
    }
  }

  async function syncImageMetadata() {
    const token = ++imageLoadToken;
    if (!imageSrc) {
      naturalWidth = 0;
      naturalHeight = 0;
      imageLoadError = false;
      onMetricsChange?.({ naturalWidth: 0, naturalHeight: 0 });
      return;
    }
    if (imageWidth > 0 && imageHeight > 0) {
      naturalWidth = imageWidth;
      naturalHeight = imageHeight;
      imageLoadError = false;
      onMetricsChange?.({ naturalWidth, naturalHeight });
      return;
    }

    naturalWidth = 0;
    naturalHeight = 0;
    imageLoadError = false;
    onMetricsChange?.({ naturalWidth: 0, naturalHeight: 0 });
    await new Promise<void>((resolve) => {
      const loader = new Image();
      loader.onload = () => {
        if (token === imageLoadToken) {
          naturalWidth = loader.naturalWidth;
          naturalHeight = loader.naturalHeight;
          imageLoadError = false;
          onMetricsChange?.({ naturalWidth, naturalHeight });
        }
        resolve();
      };
      loader.onerror = () => {
        if (token === imageLoadToken) {
          imageLoadError = true;
          naturalWidth = 0;
          naturalHeight = 0;
          onMetricsChange?.({ naturalWidth: 0, naturalHeight: 0 });
        }
        resolve();
      };
      loader.src = imageSrc;
    });
  }

  function pointerToImage(event: PointerEvent | MouseEvent): ImageRegionPoint | null {
    if (!frameElement || naturalWidth <= 0 || naturalHeight <= 0) {
      return null;
    }
    const imageRect = imageElement?.getBoundingClientRect();
    const frameRect = frameElement.getBoundingClientRect();
    const rect =
      imageRect && imageRect.width > 0 && imageRect.height > 0
        ? imageRect
        : frameRect.width > 0 && frameRect.height > 0
          ? frameRect
          : {
              left: 0,
              top: 0,
              width: naturalWidth,
              height: naturalHeight
            };
    return {
      x: clamp(Math.round(((event.clientX - rect.left) / rect.width) * naturalWidth), 0, naturalWidth),
      y: clamp(Math.round(((event.clientY - rect.top) / rect.height) * naturalHeight), 0, naturalHeight)
    };
  }

  function clearPointerInteraction(): void {
    dragState = null;
    draftPreviewRegion = null;
    if (activePointerId !== null) {
      try {
        frameElement?.releasePointerCapture(activePointerId);
      } catch {
        // Pointer capture can already be released after context-menu interruption.
      }
      activePointerId = null;
    }
  }

  function startPointer(event: PointerEvent) {
    if (event.button !== 0) {
      clearPointerInteraction();
      return;
    }
    event.preventDefault();
    if (disabled || !frameElement) {
      return;
    }
    const point = pointerToImage(event);
    if (!point) {
      return;
    }
    activePointerId = event.pointerId;
    try {
      frameElement.setPointerCapture(event.pointerId);
    } catch {
      // Pointer capture is best-effort for synthetic events and older webviews.
    }

    const hit = hitTestImageRegions(localRegions, point);
    if (hit?.kind === 'resize') {
      const region = localRegions.find((candidate) => candidate.regionId === hit.regionId);
      if (region?.regionId) {
        activeRegionId = region.regionId;
        notifySelectionForRegion(region.regionId);
        dragState = {
          kind: 'resize',
          regionId: region.regionId,
          start: point,
          origin: { x: region.x, y: region.y, width: region.width, height: region.height },
          handle: hit.handle
        };
        return;
      }
    }
    if (hit?.kind === 'move') {
      const region = localRegions.find((candidate) => candidate.regionId === hit.regionId);
      if (region?.regionId) {
        activeRegionId = region.regionId;
        notifySelectionForRegion(region.regionId);
        dragState = {
          kind: 'move',
          regionId: region.regionId,
          start: point,
          origin: { x: region.x, y: region.y, width: region.width, height: region.height }
        };
        return;
      }
    }

    if (maxRegions !== null && localRegions.length >= maxRegions) {
      notifySelection(null);
      clearPointerInteraction();
      return;
    }
    activeRegionId = null;
    notifySelection(null);
    dragState = { kind: 'draw', start: point, current: point };
  }

  function handlePointerMove(event: PointerEvent) {
    if (activePointerId !== null && event.pointerId !== activePointerId) {
      return;
    }
    if (!dragState) {
      return;
    }
    const point = pointerToImage(event);
    if (!point) {
      return;
    }
    if (dragState.kind === 'draw') {
      dragState = { ...dragState, current: point };
      draftPreviewRegion = draftImageRegion(pageNumber, dragState.start, point);
      return;
    }
    if (dragState.kind === 'move') {
      const moveState = dragState;
      const dx = point.x - moveState.start.x;
      const dy = point.y - moveState.start.y;
      localRegions = localRegions.map((region) => {
        if (region.regionId !== moveState.regionId) {
          return region;
        }
        return {
          ...region,
          x: clamp(moveState.origin.x + dx, 0, naturalWidth - moveState.origin.width),
          y: clamp(moveState.origin.y + dy, 0, naturalHeight - moveState.origin.height)
        };
      });
      return;
    }

    const resizeState = dragState;
    localRegions = localRegions.map((region) => {
      if (region.regionId !== resizeState.regionId) {
        return region;
      }
      return resizeImageRegion(region, resizeState.origin, point, resizeState.handle, {
        width: naturalWidth,
        height: naturalHeight
      });
    });
  }

  async function handlePointerUp(event?: PointerEvent) {
    if (event && activePointerId !== null && event.pointerId !== activePointerId) {
      return;
    }
    if (!dragState) {
      return;
    }
    if (dragState.kind === 'draw' && draftPreviewRegion) {
      const regionId = draftPreviewRegion.regionId ?? `region-${pageNumber}-${Date.now()}`;
      localRegions = [
        ...localRegions,
        {
          ...draftPreviewRegion,
          regionId
        }
      ];
      activeRegionId = regionId;
      notifySelection(localRegions.length - 1);
    }
    clearPointerInteraction();
    await commitRegions(localRegions);
  }

  function handleContextMenu(event: MouseEvent) {
    event.preventDefault();
    clearPointerInteraction();
    if (disabled) {
      return;
    }
    const point = pointerToImage(event);
    if (!point) {
      return;
    }
    const hit = hitTestImageRegions(localRegions, point);
    if (!hit?.regionId) {
      return;
    }
    pendingDeleteRegionId = hit.regionId;
    notifySelectionForRegion(hit.regionId);
  }

  function cancelPendingDelete() {
    pendingDeleteRegionId = null;
    clearPointerInteraction();
  }

  async function confirmPendingDelete() {
    const regionId = pendingDeleteRegionId;
    if (!regionId) {
      return;
    }
    pendingDeleteRegionId = null;
    localRegions = localRegions.filter((region) => region.regionId !== regionId);
    if (activeRegionId === regionId) {
      activeRegionId = null;
      notifySelection(null);
    }
    clearPointerInteraction();
    await commitRegions(localRegions);
  }

  async function commitRegions(nextRegions: ImageRegion[]) {
    if (onRegionsChange) {
      await onRegionsChange(nextRegions.map((region) => ({ ...region })));
    }
  }

  function notifySelectionForRegion(regionId: string) {
    notifySelection(localRegions.findIndex((region) => region.regionId === regionId));
  }

  function notifySelection(index: number | null) {
    onSelectionChange?.(index !== null && index >= 0 ? index : null);
  }

  function renderedImageMetrics(
    fw: number,
    fh: number,
    img: HTMLImageElement | null,
    _resizeTick: number
  ): { left: number; top: number; width: number; height: number } | null {
    void _resizeTick;
    if (img && img.clientWidth > 0 && img.clientHeight > 0) {
      return { left: img.offsetLeft, top: img.offsetTop, width: img.clientWidth, height: img.clientHeight };
    }
    if (fw > 0 && fh > 0) {
      return { left: 0, top: 0, width: fw, height: fh };
    }
    if (naturalWidth > 0 && naturalHeight > 0) {
      return { left: 0, top: 0, width: naturalWidth, height: naturalHeight };
    }
    return null;
  }

  function labelStyle(region: ImageRegion, index: number): string {
    const presentation = regionPresentation(region, index);
    const border = presentation.labelBorder ? `border:1px solid ${presentation.labelBorder};` : '';
    return `background:${presentation.labelBackground};color:${presentation.labelForeground};${border}`;
  }

  function handleLabel(region: ImageRegion, index: number, corner: string): string {
    const presentation = regionPresentation(region, index);
    return `${presentation.handleLabel ?? presentation.label} region ${corner} resize handle`;
  }

  $: regionStyleMap = computeRegionStyleMap(
    visibleRegions,
    renderedImageMetrics(frameWidth, frameHeight, imageElement, resizeTick),
    { width: naturalWidth, height: naturalHeight },
    regionPresentation
  );
</script>

<svelte:window
  on:pointermove={handlePointerMove}
  on:pointerup={(event) => void handlePointerUp(event)}
  on:resize={() => {
    resizeTick += 1;
  }}
/>

<div
  class={[
    'flex h-full justify-center py-2',
    constrainHeight ? 'items-center' : 'items-start',
    containerMinHeightClass
  ]}
>
  {#if imageSrc}
    {#if imageLoadError}
      <div class={`rounded-2xl border px-5 py-4 ${feedbackToneClass('error')}`}>
        <div class="text-base font-semibold">Page preview unavailable</div>
        <p class="mt-1 text-sm">The image preview could not be loaded.</p>
      </div>
    {:else}
      <InteractiveCanvasSurface
        squareCorners={squareCorners}
        class={`${constrainHeight ? 'max-h-full' : ''} max-w-full overflow-visible ${disabled ? 'cursor-wait' : 'cursor-crosshair'}`}
      >
        <div
          bind:this={frameElement}
          bind:clientWidth={frameWidth}
          bind:clientHeight={frameHeight}
          class={`relative inline-block ${constrainHeight ? 'max-h-full' : ''} max-w-full touch-none select-none ${disabled ? 'cursor-wait' : 'cursor-crosshair'}`}
          role="button"
          tabindex="0"
          aria-label={ariaLabel}
          on:pointerdown={startPointer}
          on:pointermove={handlePointerMove}
          on:pointerup={(event) => void handlePointerUp(event)}
          on:pointercancel={(event) => void handlePointerUp(event)}
          on:contextmenu={handleContextMenu}
        >
          <img
            bind:this={imageElement}
            src={imageSrc}
            alt={imageAlt}
            class={`pointer-events-none block ${constrainHeight ? 'max-h-full' : ''} max-w-full select-none`}
            draggable="false"
          />
          {#if naturalWidth > 0 && naturalHeight > 0}
            {#each visibleRegions as region, idx (regionStyleKey(region, idx))}
              <div
                class="pointer-events-none rounded-xl"
                style={regionStyleMap.get(regionStyleKey(region, idx)) ?? ''}
                >
                  <div
                    class="absolute inset-0 rounded-xl"
                    style="background:var(--workspace-selection-hatch);opacity:var(--workspace-selection-hatch-opacity);"
                  ></div>
                <div
                  class="absolute left-1 top-0.5 rounded px-1 py-0.5 text-[10px] font-semibold uppercase leading-none tracking-wider"
                  style={labelStyle(region, idx)}
                >
                  {regionPresentation(region, idx).label}
                </div>
                {#if region.regionId}
                  <SelectionHandle class="pointer-events-none left-0 top-0 size-3 -translate-x-1/2 -translate-y-1/2" label={handleLabel(region, idx, 'top-left')} />
                  <SelectionHandle class="pointer-events-none right-0 top-0 size-3 translate-x-1/2 -translate-y-1/2" label={handleLabel(region, idx, 'top-right')} />
                  <SelectionHandle class="pointer-events-none bottom-0 left-0 size-3 -translate-x-1/2 translate-y-1/2" label={handleLabel(region, idx, 'bottom-left')} />
                  <SelectionHandle class="pointer-events-none bottom-0 right-0 size-3 translate-x-1/2 translate-y-1/2" label={handleLabel(region, idx, 'bottom-right')} />
                {/if}
              </div>
            {/each}
          {/if}
        </div>
      </InteractiveCanvasSurface>
    {/if}
  {:else}
    <div class="max-w-md text-center text-base text-muted-foreground">
      No image preview is available.
    </div>
  {/if}
</div>

<ConfirmDialog
  open={pendingDeleteRegionId !== null}
  title={deleteTitle}
  description={deleteDescription}
  confirmLabel={deleteConfirmLabel}
  cancelLabel="Cancel"
  destructive
  onCancel={cancelPendingDelete}
  onConfirm={confirmPendingDelete}
/>

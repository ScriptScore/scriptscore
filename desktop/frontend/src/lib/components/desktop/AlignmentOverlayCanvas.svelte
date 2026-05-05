<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { StudentWorkflowTransform } from '$lib/types';
  import { InteractiveCanvasSurface } from './ui';

  type DragState = {
    pointerId: number;
    startX: number;
    startY: number;
    startTranslateX: number;
    startTranslateY: number;
  } | null;

  const MIN_SCALE = 0.05;
  const MAX_SCALE = 5;
  const WHEEL_SCALE_STEP = 0.005;

  export let templateImageUrl = '';
  export let submissionImageUrl = '';
  export let pageNumber = 1;
  export let transform: StudentWorkflowTransform = {
    rotation: 0,
    scale: 1,
    translateX: 0,
    translateY: 0
  };
  export let submissionOpacity = 0.6;
  export let templateTintColor = '#2563eb';
  export let busy = false;
  export let ontransformchange:
    | ((transform: StudentWorkflowTransform) => void | Promise<void>)
    | null = null;

  let workspaceElement: HTMLDivElement | null = null;
  let templateElement: HTMLImageElement | null = null;
  let submissionElement: HTMLImageElement | null = null;
  let templateNaturalWidth = 0;
  let templateRenderedWidth = 0;
  let studentNaturalWidth = 0;
  let studentNaturalHeight = 0;
  let dragState: DragState = null;
  let displayScale = 1;
  let resizeTick = 0;

  $: displayScale =
    templateNaturalWidth > 0 && templateRenderedWidth > 0
      ? templateRenderedWidth / templateNaturalWidth
      : 1;
  $: studentBaseWidth =
    studentNaturalWidth > 0 ? Math.max(1, studentNaturalWidth * displayScale) : null;
  $: studentBaseHeight =
    studentNaturalHeight > 0 ? Math.max(1, studentNaturalHeight * displayScale) : null;
  $: studentTransformStyle = [
    studentBaseWidth === null ? '' : `width:${studentBaseWidth}px;`,
    studentBaseHeight === null ? '' : `height:${studentBaseHeight}px;`,
    'display:block;',
    'max-width:none;',
    'max-height:none;',
    `opacity:${clamp(submissionOpacity, 0.2, 1)};`,
    'transform-origin:top left;',
    `transform:translate(${transform.translateX * displayScale}px, ${transform.translateY * displayScale}px) scale(${transform.scale}) rotate(${transform.rotation}deg);`
  ].join('');
  $: templateFilterStyle = `filter:${templateTintFilter(templateTintColor)};`;
  $: {
    void resizeTick;
    updateRenderedMetrics();
  }

  function updateRenderedMetrics() {
    if (!templateElement) {
      templateRenderedWidth = 0;
      return;
    }
    const rect = templateElement.getBoundingClientRect();
    templateRenderedWidth = rect.width > 0 ? rect.width : templateElement.naturalWidth;
  }

  function handleTemplateLoad() {
    templateNaturalWidth = templateElement?.naturalWidth ?? 0;
    updateRenderedMetrics();
  }

  function handleSubmissionLoad() {
    studentNaturalWidth = submissionElement?.naturalWidth ?? 0;
    studentNaturalHeight = submissionElement?.naturalHeight ?? 0;
    updateRenderedMetrics();
  }

  function startDrag(event: PointerEvent) {
    if (busy || event.button !== 0 || !workspaceElement || !submissionImageUrl) {
      return;
    }
    event.preventDefault();
    workspaceElement.setPointerCapture(event.pointerId);
    workspaceElement.style.cursor = 'grabbing';
    dragState = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      startTranslateX: transform.translateX,
      startTranslateY: transform.translateY
    };
  }

  function moveDrag(event: PointerEvent) {
    if (!dragState || event.pointerId !== dragState.pointerId) {
      return;
    }
    event.preventDefault();
    const scale = displayScale || 1;
    emitTransform({
      ...transform,
      translateX: Math.round(dragState.startTranslateX + (event.clientX - dragState.startX) / scale),
      translateY: Math.round(dragState.startTranslateY + (event.clientY - dragState.startY) / scale)
    });
  }

  function endDrag(event: PointerEvent) {
    if (!dragState || event.pointerId !== dragState.pointerId) {
      return;
    }
    workspaceElement?.releasePointerCapture(event.pointerId);
    if (workspaceElement) {
      workspaceElement.style.cursor = '';
    }
    dragState = null;
  }

  function handleWheel(event: WheelEvent) {
    if (busy || !submissionImageUrl) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const step = event.deltaY < 0 ? WHEEL_SCALE_STEP : -WHEEL_SCALE_STEP;
    emitTransform({
      ...transform,
      scale: roundScale(clamp(transform.scale + step, MIN_SCALE, MAX_SCALE))
    });
  }

  function emitTransform(next: StudentWorkflowTransform) {
    void ontransformchange?.({
      rotation: sanitizeFinite(next.rotation, transform.rotation),
      scale: roundScale(clamp(sanitizeFinite(next.scale, transform.scale), MIN_SCALE, MAX_SCALE)),
      translateX: sanitizeFinite(next.translateX, transform.translateX),
      translateY: sanitizeFinite(next.translateY, transform.translateY)
    });
  }

  function templateTintFilter(color: string): string {
    const hue = hexHue(color) ?? 215;
    const rotation = Math.round(normalizeHue(hue - 38));
    return `sepia(0.45) saturate(2.2) hue-rotate(${rotation}deg) contrast(1.05) brightness(1.02)`;
  }

  function hexHue(color: string): number | null {
    const match = /^#?([0-9a-f]{6})$/i.exec(color.trim());
    if (!match) return null;
    const value = match[1];
    const red = Number.parseInt(value.slice(0, 2), 16) / 255;
    const green = Number.parseInt(value.slice(2, 4), 16) / 255;
    const blue = Number.parseInt(value.slice(4, 6), 16) / 255;
    const max = Math.max(red, green, blue);
    const min = Math.min(red, green, blue);
    const delta = max - min;
    if (delta === 0) return 0;
    if (max === red) return normalizeHue(((green - blue) / delta) * 60);
    if (max === green) return ((blue - red) / delta + 2) * 60;
    return ((red - green) / delta + 4) * 60;
  }

  function normalizeHue(hue: number): number {
    return ((hue % 360) + 360) % 360;
  }

  function sanitizeFinite(value: number, fallback: number): number {
    return Number.isFinite(value) ? value : fallback;
  }

  function roundScale(value: number): number {
    return Math.round(value * 1000) / 1000;
  }

  function clamp(value: number, min: number, max: number): number {
    return Math.min(Math.max(value, min), max);
  }
</script>

<svelte:window on:resize={() => { resizeTick += 1; }} />

<div class="flex h-full min-h-[28rem] items-center justify-center overflow-auto bg-workspace-canvas p-8">
  {#if templateImageUrl && submissionImageUrl}
    <InteractiveCanvasSurface class="overflow-hidden">
      <div
        bind:this={workspaceElement}
        class="relative inline-block cursor-grab select-none leading-none"
        role="application"
        aria-label="Alignment overlay editor"
        on:pointerdown={startDrag}
        on:pointermove={moveDrag}
        on:pointerup={endDrag}
        on:pointercancel={endDrag}
        on:wheel={handleWheel}
        style="touch-action:none;"
      >
        <img
          bind:this={templateElement}
          src={templateImageUrl}
          alt={`Template page ${pageNumber}`}
          class="pointer-events-none block max-h-[calc(100vh-17rem)] max-w-full select-none"
          draggable="false"
          style={templateFilterStyle}
          on:load={handleTemplateLoad}
        />
        <img
          bind:this={submissionElement}
          src={submissionImageUrl}
          alt={`Submission page ${pageNumber}`}
          class="absolute left-0 top-0 select-none"
          draggable="false"
          style={studentTransformStyle}
          on:load={handleSubmissionLoad}
        />
      </div>
    </InteractiveCanvasSurface>
  {:else}
    <div class="max-w-md text-center text-sm text-workspace-text-secondary">
      Template and submission previews are required before manual alignment can continue.
    </div>
  {/if}
</div>

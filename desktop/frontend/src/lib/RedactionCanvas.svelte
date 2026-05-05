<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import ImageRegionEditor from '$lib/components/desktop/ui/ImageRegionEditor.svelte';
  import type {
    ImageRegion,
    ImageRegionMetrics,
    ImageRegionPresentation
  } from '$lib/components/desktop/ui';
  import type { TemplateRedactionRegion, TemplateRedactionRegionInput } from '$lib/types';
  import {
    REDACTION_LABEL_NAME_IDENTIFICATION,
    REDACTION_LABEL_PRIVACY_PROTECTION
  } from '$lib/types';

  export let imageUrl = '';
  export let pageNumber = 1;
  export let regions: TemplateRedactionRegion[] = [];
  export let examRegionCount = regions.length;
  export let busy = false;
  export let squarePageCorners = false;
  export let editorMinHeightClass = 'min-h-[28rem]';
  export let editorConstrainHeight = true;
  export let onRegionsChange: ((regions: TemplateRedactionRegionInput[]) => void | Promise<void>) | null =
    null;
  export let onMetricsChange: ((metrics: ImageRegionMetrics) => void) | null = null;

  $: editorRegions = regions.map(toEditorRegion);

  function toEditorRegion(region: TemplateRedactionRegion, index: number): ImageRegion {
    return {
      regionId: region.regionId,
      pageNumber: region.pageNumber,
      x: region.x,
      y: region.y,
      width: region.width,
      height: region.height,
      kind: redactionKind(region, index)
    };
  }

  function redactionKind(region: TemplateRedactionRegion, index: number): string {
    if (region.label === REDACTION_LABEL_NAME_IDENTIFICATION || region.sortOrder === 0) {
      return REDACTION_LABEL_NAME_IDENTIFICATION;
    }
    if (region.label === REDACTION_LABEL_PRIVACY_PROTECTION) {
      return REDACTION_LABEL_PRIVACY_PROTECTION;
    }
    return examRegionCount === 0 && index === 0
      ? REDACTION_LABEL_NAME_IDENTIFICATION
      : REDACTION_LABEL_PRIVACY_PROTECTION;
  }

  function redactionPresentation(region: ImageRegion): ImageRegionPresentation {
    const isName = region.kind === REDACTION_LABEL_NAME_IDENTIFICATION;
    return {
      label: isName ? 'Name' : 'Privacy',
      borderColor: isName
        ? 'var(--workspace-selection-name-border)'
        : 'var(--workspace-selection-border)',
      fillColor: isName
        ? 'var(--workspace-selection-name-fill)'
        : 'var(--workspace-selection-fill)',
      labelBackground: isName
        ? 'var(--workspace-selection-name-border)'
        : 'var(--workspace-selection-handle-fill)',
      labelForeground: isName
        ? 'var(--workspace-selection-name-foreground)'
        : 'var(--workspace-selection-handle-border)',
      labelBorder: isName ? undefined : 'var(--workspace-selection-handle-border)'
    };
  }

  async function handleRegionsChange(nextRegions: ImageRegion[]) {
    await onRegionsChange?.(
      nextRegions.map((region) => ({
        regionId: region.regionId,
        pageNumber: region.pageNumber,
        x: region.x,
        y: region.y,
        width: region.width,
        height: region.height
      }))
    );
  }
</script>

{#if imageUrl}
  <ImageRegionEditor
    imageSrc={imageUrl}
    imageAlt={`Template page ${pageNumber}`}
    {pageNumber}
    regions={editorRegions}
    disabled={busy}
    ariaLabel="Draw redaction region"
    squareCorners={squarePageCorners}
    regionPresentation={redactionPresentation}
    deleteTitle="Delete redaction region?"
    deleteDescription="Remove this redaction region from the page."
    containerMinHeightClass={editorMinHeightClass}
    constrainHeight={editorConstrainHeight}
    onRegionsChange={handleRegionsChange}
    {onMetricsChange}
  />
{:else}
  <div class="flex h-full min-h-[28rem] items-center justify-center py-2">
    <div class="max-w-md text-center text-base text-muted-foreground">
      Choose a template PDF to run setup, or open a project with existing template pages.
    </div>
  </div>
{/if}

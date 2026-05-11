<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onMount } from 'svelte';

  import { getLegalDisclosure, isDesktopHost } from '$lib/desktop';
  import type { LegalDisclosure } from '$lib/types';
  import { DesktopButton, Surface } from './ui';

  type InventoryRow = {
    name: string;
    version: string;
    license: string;
    licenseNote?: string;
    licenseNoteId?: number;
    source: string;
    scope: string;
  };

  type NoticeBlock =
    | {
        kind: 'heading1' | 'heading2' | 'paragraph' | 'bullet' | 'pre';
        text: string;
      }
    | {
        kind: 'inventory';
        rows: InventoryRow[];
      };

  type NoticeLine =
    | { kind: 'blank' }
    | { kind: 'bullet'; text: string }
    | { kind: 'heading'; level: 1 | 2; text: string }
    | { kind: 'text'; text: string };

  type NoticeParseState = {
    blocks: NoticeBlock[];
    paragraph: string[];
    pre: string[];
    collectingInventory: boolean;
    skippingReviewFindings: boolean;
  };

  const fallbackSourceUrl = 'https://github.com/ScriptScore/scriptscore';

  let disclosure: LegalDisclosure | null = null;
  let noticesExpanded = false;

  $: hasBundledNotices = disclosure?.artifactStatus === 'bundled';
  $: sourceUrl = disclosure?.sourceUrl ?? fallbackSourceUrl;
  $: noticeBlocks = renderNoticeMarkdown(disclosure?.thirdPartyNotices ?? '');

  onMount(() => {
    void getLegalDisclosure().then((value) => {
      disclosure = value;
    });
  });

  async function openSourceUrl(event: MouseEvent) {
    if (!isDesktopHost()) {
      return;
    }
    event.preventDefault();
    try {
      const shell = await import('@tauri-apps/plugin-shell');
      await shell.open(sourceUrl);
    } catch {
      window.open(sourceUrl, '_blank', 'noopener,noreferrer');
    }
  }

  function parseNoticeLine(line: string): NoticeLine {
    const trimmed = line.trim();
    if (!trimmed) {
      return { kind: 'blank' };
    }
    if (line.startsWith('## ')) {
      return { kind: 'heading', level: 2, text: line.slice(3).trim() };
    }
    if (line.startsWith('# ')) {
      return { kind: 'heading', level: 1, text: line.slice(2).trim() };
    }
    if (line.startsWith('- ')) {
      return { kind: 'bullet', text: line.slice(2).trim() };
    }
    return { kind: 'text', text: trimmed };
  }

  function splitNoticeLines(markdown: string): string[] {
    return markdown.replaceAll('\r\n', '\n').replaceAll('\r', '\n').split('\n');
  }

  function flushParagraphBlock(blocks: NoticeBlock[], paragraph: string[]) {
    const text = paragraph.join(' ').trim();
    if (text) {
      blocks.push({ kind: 'paragraph', text });
    }
    paragraph.length = 0;
  }

  function flushPreBlock(blocks: NoticeBlock[], pre: string[]) {
    const text = pre.join('\n').trim();
    if (text) {
      blocks.push({ kind: 'pre', text });
    }
    pre.length = 0;
  }

  function flushInventoryBlock(blocks: NoticeBlock[], pre: string[]) {
    const text = pre.join('\n').trim();
    if (text) {
      const rows = parseInventoryCsv(text);
      blocks.push(rows.length > 0 ? { kind: 'inventory', rows } : { kind: 'pre', text });
    }
    pre.length = 0;
  }

  function flushCollectedBlocks(
    blocks: NoticeBlock[],
    paragraph: string[],
    pre: string[],
    collectingInventory: boolean
  ) {
    flushParagraphBlock(blocks, paragraph);
    if (collectingInventory) {
      flushInventoryBlock(blocks, pre);
    } else {
      flushPreBlock(blocks, pre);
    }
  }

  function handleNoticeHeading(state: NoticeParseState, line: Extract<NoticeLine, { kind: 'heading' }>) {
    flushCollectedBlocks(state.blocks, state.paragraph, state.pre, state.collectingInventory);
    state.collectingInventory = line.text === 'Inventory Summary';
    state.skippingReviewFindings = line.text === 'Review Findings';
    if (!state.skippingReviewFindings) {
      state.blocks.push({
        kind: line.level === 1 ? 'heading1' : 'heading2',
        text: line.text
      });
    }
  }

  function appendNoticeLine(state: NoticeParseState, rawLine: string, line: NoticeLine) {
    if (state.skippingReviewFindings) {
      return;
    }

    if (state.collectingInventory) {
      state.pre.push(rawLine);
      return;
    }

    if (line.kind === 'bullet') {
      flushParagraphBlock(state.blocks, state.paragraph);
      state.blocks.push({ kind: 'bullet', text: line.text });
      return;
    }

    if (line.kind === 'blank') {
      flushParagraphBlock(state.blocks, state.paragraph);
      return;
    }

    if (line.kind === 'text') {
      state.paragraph.push(line.text);
    }
  }

  function renderNoticeMarkdown(markdown: string): NoticeBlock[] {
    const state: NoticeParseState = {
      blocks: [],
      paragraph: [],
      pre: [],
      collectingInventory: false,
      skippingReviewFindings: false
    };

    for (const rawLine of splitNoticeLines(markdown)) {
      const line = parseNoticeLine(rawLine);

      if (line.kind === 'heading') {
        handleNoticeHeading(state, line);
        continue;
      }

      appendNoticeLine(state, rawLine, line);
    }

    flushCollectedBlocks(state.blocks, state.paragraph, state.pre, state.collectingInventory);
    return state.blocks;
  }

  function parseInventoryCsv(csvText: string): InventoryRow[] {
    const records: string[][] = [];
    let row: string[] = [];
    let field = '';
    let quoted = false;

    function pushField() {
      row.push(field);
      field = '';
    }

    function pushRow() {
      pushField();
      if (row.some((value) => value.trim().length > 0)) {
        records.push(row.map((value) => value.trim()));
      }
      row = [];
    }

    for (let index = 0; index < csvText.length; index += 1) {
      const char = csvText[index];
      if (quoted) {
        if (char === '"' && csvText[index + 1] === '"') {
          field += '"';
          index += 1;
        } else if (char === '"') {
          quoted = false;
        } else {
          field += char;
        }
      } else if (char === '"') {
        quoted = true;
      } else if (char === ',') {
        pushField();
      } else if (char === '\n') {
        pushRow();
      } else if (char !== '\r') {
        field += char;
      }
    }
    if (field.length > 0 || row.length > 0) {
      pushRow();
    }

    let nextLicenseNoteId = 0;
    return records.slice(1).map((record) => {
      const rawLicense = record[2] ?? '';
      const license = summarizeLicense(rawLicense);
      const needsLicenseNote = Boolean(rawLicense) && license !== rawLicense;
      let licenseNoteId: number | undefined;
      if (needsLicenseNote) {
        nextLicenseNoteId += 1;
        licenseNoteId = nextLicenseNoteId;
      }
      return {
        name: record[0] ?? '',
        version: record[1] ?? '',
        license,
        licenseNote: needsLicenseNote ? rawLicense : undefined,
        licenseNoteId,
        source: record[3] ?? '',
        scope: record[4] ?? ''
      };
    });
  }

  function displayInventoryVersion(row: InventoryRow): string {
    return row.version || 'Not applicable';
  }

  function displayInventoryLicense(row: InventoryRow): string {
    if (!row.license) {
      return row.scope === 'frontend-asset' || row.scope === 'model-asset' || row.scope === 'native-library'
        ? 'Release review required'
        : 'Not specified';
    }
    return row.license;
  }

  function summarizeLicense(license: string): string {
    if (license.length <= 120) {
      return license;
    }
    if (/^BSD 3-Clause License/i.test(license)) {
      return 'BSD-3-Clause';
    }
    if (/^MIT License/i.test(license) || /The MIT License/i.test(license)) {
      return 'MIT';
    }
    if (/^Apache License Version 2\.0/i.test(license)) {
      return 'Apache-2.0';
    }
    return `${license.slice(0, 117).trim()}...`;
  }

  function licenseNoteRows(rows: InventoryRow[]): InventoryRow[] {
    return rows.filter((row) => row.licenseNote && row.licenseNoteId);
  }
</script>

<section class="grid gap-5">
  <div>
    <h2 class="shell-title font-medium text-workspace-text-primary">
      Source Code and Open Source Licenses
    </h2>
    <p class="mt-2 shell-body leading-6 text-workspace-text-secondary">
      ScriptScore Desktop and its bundled open-core runtime are covered by the GNU Affero
      General Public License version 3 only. The program is provided without warranty except
      where expressly stated in writing.
    </p>
  </div>

  <div class="grid gap-3 md:grid-cols-2">
    <Surface variant="cardControl" bordered radius="md" class="min-w-0 p-4">
      <div class="shell-eyebrow text-workspace-text-muted">Corresponding Source</div>
      <!-- eslint-disable svelte/no-navigation-without-resolve -->
      <a
        class="mt-2 inline-flex max-w-full text-sm font-medium text-workspace-text-primary underline underline-offset-4 transition-colors hover:text-workspace-text-secondary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
        href={sourceUrl}
        target="_blank"
        rel="noreferrer"
        onclick={(event) => void openSourceUrl(event)}
      >
        <span class="truncate">{sourceUrl}</span>
      </a>
      <!-- eslint-enable svelte/no-navigation-without-resolve -->
    </Surface>
    <Surface variant="cardControl" bordered radius="md" class="min-w-0 p-4">
      <div class="shell-eyebrow text-workspace-text-muted">License</div>
      <div class="mt-2 shell-body font-medium text-workspace-text-primary">
        {disclosure?.licenseExpression ?? 'AGPL-3.0-only'}
      </div>
    </Surface>
  </div>

  <Surface variant="message" bordered radius="md" class="min-w-0 p-4">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0 flex-1">
        <div class="shell-eyebrow text-workspace-text-muted">
          {hasBundledNotices ? 'Bundled Third-Party Notices' : 'Dev/Test Fallback'}
        </div>
        <div class="mt-2 shell-body leading-6 text-workspace-text-secondary">
          {#if !disclosure}
            Loading legal artifacts...
          {:else if hasBundledNotices}
            Release packages include generated dependency notices from the bundled legal resources.
          {:else}
            {disclosure.thirdPartyNotices}
          {/if}
        </div>
      </div>
      {#if hasBundledNotices}
        <DesktopButton
          class="min-w-32 shrink-0 whitespace-nowrap"
          size="compact"
          aria-expanded={noticesExpanded}
          onclick={() => {
            noticesExpanded = !noticesExpanded;
          }}
        >
          {noticesExpanded ? 'Hide notices' : 'Show notices'}
        </DesktopButton>
      {/if}
    </div>
    {#if hasBundledNotices && noticesExpanded}
      <div class="mt-4 max-h-72 min-w-0 overflow-auto pr-2">
        <div class="grid gap-3 text-workspace-text-secondary">
          {#each noticeBlocks as block, index (`${block.kind}-${index}`)}
            {#if block.kind === 'heading1'}
              <h3 class="shell-title font-medium text-workspace-text-primary">{block.text}</h3>
            {:else if block.kind === 'heading2'}
              <h4 class="shell-section-title text-workspace-text-primary">{block.text}</h4>
            {:else if block.kind === 'bullet'}
              <div class="flex gap-2 shell-body leading-6">
                <span aria-hidden="true">-</span>
                <span>{block.text}</span>
              </div>
            {:else if block.kind === 'inventory'}
              {@const licenseNotes = licenseNoteRows(block.rows)}
              <div class="grid gap-px overflow-hidden rounded-md border border-border-default bg-border-default">
                <div class="hidden grid-cols-[minmax(0,1.25fr)_minmax(0,0.55fr)_minmax(0,1.15fr)_minmax(0,0.65fr)_minmax(0,0.8fr)] gap-3 bg-surface-card-control px-3 py-2 shell-eyebrow text-workspace-text-muted md:grid">
                  <span>Name</span>
                  <span>Version</span>
                  <span>License</span>
                  <span>Source</span>
                  <span>Scope</span>
                </div>
                {#each block.rows as row, rowIndex (`${row.name}-${row.version}-${rowIndex}`)}
                  <div class="grid grid-cols-1 gap-2 bg-surface-message px-3 py-2 shell-body leading-5 md:grid-cols-[minmax(0,1.25fr)_minmax(0,0.55fr)_minmax(0,1.15fr)_minmax(0,0.65fr)_minmax(0,0.8fr)] md:gap-3">
                    <div class="min-w-0 break-words">
                      <span class="shell-eyebrow text-workspace-text-muted md:hidden">Name</span>
                      <span class="block text-workspace-text-primary">{row.name}</span>
                    </div>
                    <div class="min-w-0 break-words">
                      <span class="shell-eyebrow text-workspace-text-muted md:hidden">Version</span>
                      <span class="block">{displayInventoryVersion(row)}</span>
                    </div>
                    <div class="min-w-0 break-words">
                      <span class="shell-eyebrow text-workspace-text-muted md:hidden">License</span>
                      <span class="block">
                        {displayInventoryLicense(row)}
                        {#if row.licenseNoteId}
                          <sup class="ml-1 text-workspace-text-muted">[{row.licenseNoteId}]</sup>
                        {/if}
                      </span>
                    </div>
                    <div class="min-w-0 break-words">
                      <span class="shell-eyebrow text-workspace-text-muted md:hidden">Source</span>
                      <span class="block">{row.source}</span>
                    </div>
                    <div class="min-w-0 break-words">
                      <span class="shell-eyebrow text-workspace-text-muted md:hidden">Scope</span>
                      <span class="block">{row.scope}</span>
                    </div>
                  </div>
                {/each}
                {#if licenseNotes.length > 0}
                  <div class="grid gap-2 bg-surface-message px-3 py-3 shell-body leading-5 text-workspace-text-secondary">
                    <div class="shell-eyebrow text-workspace-text-muted">License Notes</div>
                    {#each licenseNotes as row (`license-note-${row.licenseNoteId}`)}
                      <div class="grid gap-1">
                        <div class="font-medium text-workspace-text-primary">
                          [{row.licenseNoteId}] {row.name}
                        </div>
                        <div class="break-words">{row.licenseNote}</div>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            {:else if block.kind === 'pre'}
              <pre class="max-w-full whitespace-pre-wrap break-words shell-body leading-6 text-workspace-text-secondary">{block.text}</pre>
            {:else}
              <p class="shell-body leading-6">{block.text}</p>
            {/if}
          {/each}
        </div>
      </div>
    {/if}
  </Surface>
</section>

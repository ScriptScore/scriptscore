<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { AppSettings } from '$lib/types';
  import { SelectField } from './ui';

  export let settings: AppSettings;
  export let onChange: (() => void | Promise<void>) | null = null;

  const workerOptions = [
    { value: '1', label: '1', subitem: 'Safest default serial behavior.' },
    { value: '2', label: '2', subitem: 'Run two answer rows at a time.' },
    { value: '3', label: '3', subitem: 'Run three answer rows at a time.' },
    { value: '4', label: '4', subitem: 'Run four answer rows at a time.' }
  ];

  function setWorkerCount(value: string) {
    const parsed = Number(value);
    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 4) {
      return;
    }
    settings.preliminaryGradingMaxWorkers = parsed;
    void onChange?.();
  }
</script>

<section class="space-y-2">
  <SelectField
    class="max-w-xs"
    label="Preliminary grading workers"
    value={String(settings.preliminaryGradingMaxWorkers)}
    options={workerOptions}
    showSubitems
    onChange={setWorkerCount}
  />
  <div class="text-xs leading-6 text-workspace-text-muted">
    Applies only to preliminary scoring. Higher values may improve throughput on some local runtimes.
  </div>
</section>

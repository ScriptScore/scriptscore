<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { AppSettings } from '$lib/types';
  import { ToggleRow } from './ui';

  export let settings: AppSettings;
  export let onChange: (() => void | Promise<void>) | null = null;
</script>

<section class="space-y-2">
  <ToggleRow
    title="Dark theme"
    description="Save the preferred workspace theme for the desktop shell."
    checked={settings.theme === 'dark'}
    onToggle={() => {
      settings.theme = settings.theme === 'dark' ? 'light' : 'dark';
      void onChange?.();
    }}
  />
  <div class="text-xs text-workspace-text-muted">
    {settings.theme === 'dark' ? 'Dark mode selected' : 'Light mode selected'}
  </div>
</section>

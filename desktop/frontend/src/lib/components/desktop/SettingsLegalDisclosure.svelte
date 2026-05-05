<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onMount } from 'svelte';

  import { getLegalDisclosure } from '$lib/desktop';
  import type { LegalDisclosure } from '$lib/types';
  import { Surface } from './ui';

  let disclosure: LegalDisclosure | null = null;

  onMount(() => {
    void getLegalDisclosure().then((value) => {
      disclosure = value;
    });
  });
</script>

<Surface as="section" variant="cardSubtle" bordered radius="lg" class="grid gap-5 p-6">
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
      <div class="mt-2 break-words shell-body font-medium text-workspace-text-primary">
        {disclosure?.sourceUrl ?? 'https://github.com/ScriptScore/scriptscore'}
      </div>
    </Surface>
    <Surface variant="cardControl" bordered radius="md" class="min-w-0 p-4">
      <div class="shell-eyebrow text-workspace-text-muted">Third-Party Notices</div>
      <div class="mt-2 break-words shell-body font-medium text-workspace-text-primary">
        {disclosure?.localNoticesPath ?? 'desktop/dist/legal/THIRD_PARTY_NOTICES.md'}
      </div>
    </Surface>
  </div>

  <div class="grid gap-2">
    <div class="shell-eyebrow text-workspace-text-muted">License</div>
    <div class="shell-body text-workspace-text-primary">
      {disclosure?.licenseExpression ?? 'AGPL-3.0-only'}
    </div>
  </div>

  <Surface variant="message" bordered radius="md" class="p-4">
    <div class="shell-eyebrow text-workspace-text-muted">
      {disclosure?.artifactStatus === 'bundled' ? 'Bundled Notices' : 'Dev/Test Fallback'}
    </div>
    <pre class="mt-2 max-h-72 overflow-auto whitespace-pre-wrap shell-body leading-6 text-workspace-text-secondary">{disclosure?.thirdPartyNotices ?? 'Loading legal artifacts...'}</pre>
  </Surface>
</Surface>

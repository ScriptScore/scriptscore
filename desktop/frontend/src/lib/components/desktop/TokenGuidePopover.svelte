<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { DesktopPopover } from './ui';

  export let label = 'Not sure how?';
  export let imageSrc: string;
  export let imageAlt: string;
  export let open = false;
</script>

<DesktopPopover
  bind:open
  rootClass="relative inline-block"
  triggerClass="text-sm font-medium text-primary underline-offset-4 hover:underline"
  triggerLabel={label}
  triggerAriaHaspopup="dialog"
  panelRole="dialog"
  panelAriaLabel={imageAlt}
  panelClass="bottom-full left-0 mb-2 mt-0 w-max max-w-[calc(100vw-4rem)] rounded-xl p-1"
>
  <svelte:fragment slot="trigger">
    {label}
  </svelte:fragment>
  <img
    class="block h-auto max-h-[min(46vh,30rem)] max-w-[min(54vw,42rem)] rounded-lg object-contain"
    src={imageSrc}
    alt={imageAlt}
  />
</DesktopPopover>

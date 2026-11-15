<script lang="ts">
  let { fs, size = 26 }: { fs: string; size?: number } = $props();
  const net = $derived(["smbfs", "smb", "nfs", "afpfs", "cifs", "webdav"].includes(fs.toLowerCase()));
  const internal = $derived(["apfs", "hfs"].includes(fs.toLowerCase()));
</script>

<svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true" style="flex:none">
  {#if net}
    <rect x="4" y="5" width="24" height="8" rx="2" fill="var(--bg-3)" stroke="var(--fg-2)" />
    <rect x="4" y="15" width="24" height="8" rx="2" fill="var(--bg-3)" stroke="var(--fg-2)" />
    <circle cx="8" cy="9" r="1.2" fill="var(--accent)" /><circle cx="8" cy="19" r="1.2" fill="var(--accent)" />
    <path d="M16 23v3M10 27h12" stroke="var(--fg-2)" stroke-width="1.6" stroke-linecap="round" />
  {:else if internal}
    <rect x="3.5" y="8" width="25" height="16" rx="3" fill="var(--bg-3)" stroke="var(--fg-2)" />
    <circle cx="23" cy="17" r="1.5" fill="var(--accent)" />
    <path d="M8 17h8" stroke="var(--fg-2)" stroke-width="1.6" stroke-linecap="round" />
  {:else}
    <rect x="5" y="6" width="22" height="20" rx="3.5" fill="var(--bg-3)" stroke="var(--fg-2)" />
    <circle cx="16" cy="15" r="5" fill="none" stroke="var(--fg-2)" />
    <circle cx="16" cy="15" r="1.4" fill="var(--accent)" />
    <circle cx="22.5" cy="22" r="1.1" fill="var(--fg-2)" />
  {/if}
</svg>

<script lang="ts">
  import type { Kind } from "$lib/types";
  let { kind, size = 16 }: { kind: Kind; size?: number } = $props();
  const colors: Record<Kind, string> = {
    folder: "#e0a526", video: "#d9534f", image: "#3c9d6b", audio: "#d9772b", document: "#4a86d8", archive: "#8b6ad1", other: "#8a8798",
  };
</script>

<svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true" style="flex:none">
  {#if kind === "folder"}
    <path d="M1.5 4.2c0-.7.6-1.2 1.2-1.2h3l1.3 1.4h6.3c.7 0 1.2.5 1.2 1.2v6.2c0 .7-.5 1.2-1.2 1.2H2.7c-.7 0-1.2-.5-1.2-1.2z" fill={colors.folder} />
  {:else if kind === "video"}
    <rect x="1.5" y="3" width="13" height="10" rx="1.8" fill={colors.video} />
    <path d="M6.6 5.7v4.6l3.9-2.3z" fill="#fff" />
  {:else if kind === "image"}
    <rect x="1.5" y="2.5" width="13" height="11" rx="1.8" fill={colors.image} />
    <circle cx="5.3" cy="6" r="1.3" fill="#fff" />
    <path d="M2.5 12.5l3.7-3.6 2.6 2.4 2-1.9 3.2 3.1z" fill="#fff" opacity=".9" />
  {:else if kind === "audio"}
    <rect x="1.5" y="2.5" width="13" height="11" rx="1.8" fill={colors.audio} />
    <path d="M6 10.8V5.4l5-1v5.2" fill="none" stroke="#fff" stroke-width="1.2" />
    <circle cx="5" cy="10.9" r="1.3" fill="#fff" /><circle cx="10" cy="9.8" r="1.3" fill="#fff" />
  {:else if kind === "archive"}
    <rect x="2.5" y="1.5" width="11" height="13" rx="1.6" fill={colors.archive} />
    <path d="M8 2v7M7 3.4h2M7 5h2M7 6.6h2" stroke="#fff" stroke-width="1" />
    <rect x="6.4" y="9" width="3.2" height="3.4" rx=".7" fill="#fff" />
  {:else if kind === "document"}
    <path d="M3.5 1.5h6l3 3v10h-9z" fill={colors.document} />
    <path d="M9.5 1.5v3h3" fill="#fff" opacity=".45" />
    <path d="M5.4 8h5.2M5.4 10h5.2M5.4 12h3.2" stroke="#fff" stroke-width="1" />
  {:else}
    <path d="M3.5 1.5h6l3 3v10h-9z" fill={colors.other} />
    <path d="M9.5 1.5v3h3" fill="#fff" opacity=".45" />
  {/if}
</svg>

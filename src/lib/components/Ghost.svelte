<script lang="ts">
  let { size = 120 }: { size?: number } = $props();
</script>

<svg width={size} height={size * 1.1} viewBox="0 0 120 132" aria-hidden="true">
  <ellipse cx="60" cy="122" rx="30" ry="5" fill="var(--bg-3)" />
  <path d="M20 112V56C20 31 37 14 60 14s40 17 40 42v56l-13-9-13 9-14-9-14 9-13-9z"
        fill="var(--bg)" stroke="var(--accent)" stroke-width="3" stroke-linejoin="round" />
  <ellipse cx="46" cy="55" rx="5" ry="7" fill="var(--accent)" />
  <ellipse cx="74" cy="55" rx="5" ry="7" fill="var(--accent)" />
  <ellipse cx="60" cy="74" rx="6" ry="4.5" fill="var(--accent)" opacity=".7" />
  <circle cx="35" cy="68" r="4" fill="var(--accent)" opacity=".18" />
  <circle cx="85" cy="68" r="4" fill="var(--accent)" opacity=".18" />
</svg>

<script setup lang="ts">
defineProps<{
  kind: string;
}>();
</script>

<template>
  <svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true">
    <!-- Dashboard: overview grid -->
    <g v-if="kind === 'dashboard'" fill="none" stroke="currentColor" stroke-width="1.75">
      <rect x="3" y="3" width="8" height="8" rx="1.5" />
      <rect x="13" y="3" width="8" height="8" rx="1.5" />
      <rect x="3" y="13" width="8" height="8" rx="1.5" />
      <rect x="13" y="13" width="8" height="8" rx="1.5" />
    </g>
    <!-- Profiles: stacked saved snapshots -->
    <g
      v-else-if="kind === 'profiles'"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M12 3.5l8 4.25-8 4.25-8-4.25 8-4.25z" />
      <path d="M4 12l8 4.25L20 12" />
      <path d="M4 15.75L12 20l8-4.25" />
    </g>
    <!-- Rules: automated check -->
    <g v-else-if="kind === 'rules'" fill="none" stroke="currentColor" stroke-width="1.75">
      <rect x="3" y="3" width="18" height="18" rx="2.5" />
      <path d="M8 12.5l2.5 2.5L16 9.5" stroke-linecap="round" stroke-linejoin="round" />
    </g>
    <!-- Routing: two sources merging into one path -->
    <g v-else-if="kind === 'routing'">
      <circle cx="5" cy="6" r="2" fill="currentColor" />
      <circle cx="5" cy="18" r="2" fill="currentColor" />
      <circle cx="19" cy="12" r="2" fill="currentColor" />
      <path
        d="M7 6h5a4 4 0 0 1 4 4M7 18h5a4 4 0 0 0 4-4"
        fill="none"
        stroke="currentColor"
        stroke-width="1.75"
        stroke-linecap="round"
      />
    </g>
    <!-- Mixer: channel faders -->
    <g v-else-if="kind === 'mixer'">
      <path
        d="M6 3v18M12 3v18M18 3v18"
        fill="none"
        stroke="currentColor"
        stroke-width="1.75"
        stroke-linecap="round"
      />
      <rect x="4" y="14" width="4" height="3" rx="1" fill="currentColor" />
      <rect x="10" y="6" width="4" height="3" rx="1" fill="currentColor" />
      <rect x="16" y="10" width="4" height="3" rx="1" fill="currentColor" />
    </g>
    <!-- Sources: capture microphone -->
    <path
      v-else-if="kind === 'sources'"
      d="M12 3a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0M12 19v2"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <!-- Effects: processing curve -->
    <path
      v-else-if="kind === 'effects'"
      d="M3 13c2.5-6 5.5-6 8-1s5.5 5 8-1"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <!-- Settings: gear -->
    <g v-else-if="kind === 'settings'" fill="none" stroke="currentColor" stroke-width="1.6">
      <circle cx="12" cy="12" r="3" />
      <path
        d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </g>
  </svg>
</template>

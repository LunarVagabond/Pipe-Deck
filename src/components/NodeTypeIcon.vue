<script setup lang="ts">
defineProps<{
  kind: string;
}>();
</script>

<template>
  <svg class="node-type-icon" viewBox="0 0 24 24" aria-hidden="true">
    <!-- Playback stream: sends audio out -->
    <path
      v-if="kind === 'playback'"
      d="M6 4l14 8-14 8V4z"
      fill="currentColor"
    />
    <!-- Capture stream: pulls audio in -->
    <path
      v-else-if="kind === 'capture'"
      d="M12 3a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0M12 19v2"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <!-- Physical output: speaker -->
    <path
      v-else-if="kind === 'output'"
      d="M4 9h4l5-4v14l-5-4H4V9zM16.5 8.5a4.5 4.5 0 0 1 0 7"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <!-- Physical input: hardware mic -->
    <path
      v-else-if="kind === 'input'"
      d="M12 3a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0M12 19v2"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <!-- Virtual sink: fans one signal out to several -->
    <path
      v-else-if="kind === 'virtual-sink'"
      d="M4 12h5m0 0l-2.5-2.5M9 12l-2.5 2.5M11 6h5m3 0-3-2v4l3-2zM11 18h5m3 0-3-2v4l3-2z"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <!-- Virtual input: mixes several sources into one -->
    <path
      v-else-if="kind === 'virtual-input'"
      d="M5 6h4m3 0-3-2v4l3-2zM5 18h4m3 0-3-2v4l3-2zM12 12h5m0 0-2.5-2.5M17 12l-2.5 2.5"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
</template>

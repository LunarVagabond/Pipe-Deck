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
    <!-- Virtual sink / Fan-Out node: fans one signal out to several -->
    <path
      v-else-if="kind === 'virtual-sink' || kind === 'fan_out'"
      d="M4 12h5m0 0l-2.5-2.5M9 12l-2.5 2.5M11 6h5m3 0-3-2v4l3-2zM11 18h5m3 0-3-2v4l3-2z"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <!-- Mixer node: fader sliders on a console -->
    <g v-else-if="kind === 'mixer'" stroke="currentColor" stroke-width="1.75" stroke-linecap="round">
      <path d="M6 5v14M12 5v14M18 5v14" />
      <rect x="4.4" y="13.5" width="3.2" height="2.6" rx="0.7" fill="currentColor" stroke="none" />
      <rect x="10.4" y="7.5" width="3.2" height="2.6" rx="0.7" fill="currentColor" stroke="none" />
      <rect x="16.4" y="10.5" width="3.2" height="2.6" rx="0.7" fill="currentColor" stroke="none" />
    </g>
    <!-- 5-Band EQ node: graphic equalizer bars -->
    <g v-else-if="kind === 'eq5band'" fill="currentColor">
      <rect x="2.4" y="12" width="2.3" height="7" rx="0.6" />
      <rect x="6.75" y="8" width="2.3" height="11" rx="0.6" />
      <rect x="11.1" y="4" width="2.3" height="15" rx="0.6" />
      <rect x="15.45" y="9" width="2.3" height="10" rx="0.6" />
      <rect x="19.8" y="6.5" width="2.3" height="12.5" rx="0.6" />
    </g>
    <!-- Delay node: decaying echo pulses -->
    <g v-else-if="kind === 'delay'" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round">
      <path d="M4 12a8 8 0 1 1 2.3 5.6" />
      <path d="M4 12v5h5" />
      <path d="M12 8v4l2.5 2.5" stroke-width="1.5" />
    </g>
    <!-- Stub effect node: not implemented yet -->
    <circle
      v-else-if="kind === 'stub'"
      cx="12"
      cy="12"
      r="7"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-dasharray="3 3.2"
    />
    <!-- Terminal virtual output: signal flows in and stops — no fan-out branches -->
    <path
      v-else-if="kind === 'virtual-output'"
      d="M4 12h11m0 0-2.5-2.5M15 12l-2.5 2.5M18 7v10"
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
    <!-- Bluetooth device: classic bluetooth rune -->
    <path
      v-else-if="kind === 'bluetooth'"
      d="M7 8.5 17 15l-5 3.5v-13L17 9 7 15.5"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <!-- Unknown/fallback: neutral dot -->
    <circle v-else cx="12" cy="12" r="3" fill="currentColor" />
  </svg>
</template>

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
    <!-- Group node: one input feeding several bundled outputs, drawn
         inside a dashed bracket so it reads as "a container" rather than
         a plain Fan-Out (issue #80, PD-035) -->
    <g
      v-else-if="kind === 'group'"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M2.5 12h3.5" />
      <rect
        x="8"
        y="4"
        width="14"
        height="16"
        rx="2.5"
        stroke-dasharray="2.5 2"
      />
      <path d="M9.5 8h2M9.5 12h2M9.5 16h2" />
      <circle cx="16.5" cy="8" r="1.6" fill="currentColor" stroke="none" />
      <circle cx="16.5" cy="12" r="1.6" fill="currentColor" stroke="none" />
      <circle cx="16.5" cy="16" r="1.6" fill="currentColor" stroke="none" />
    </g>
    <!-- Mixer node: fader sliders on a console -->
    <g
      v-else-if="kind === 'mixer'"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
    >
      <path d="M6 5v14M12 5v14M18 5v14" />
      <rect
        x="4.4"
        y="13.5"
        width="3.2"
        height="2.6"
        rx="0.7"
        fill="currentColor"
        stroke="none"
      />
      <rect
        x="10.4"
        y="7.5"
        width="3.2"
        height="2.6"
        rx="0.7"
        fill="currentColor"
        stroke="none"
      />
      <rect
        x="16.4"
        y="10.5"
        width="3.2"
        height="2.6"
        rx="0.7"
        fill="currentColor"
        stroke="none"
      />
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
    <g
      v-else-if="kind === 'delay'"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
    >
      <path d="M4 12a8 8 0 1 1 2.3 5.6" />
      <path d="M4 12v5h5" />
      <path d="M12 8v4l2.5 2.5" stroke-width="1.5" />
    </g>
    <!-- Limiter node: a waveform clipped flat at a ceiling/floor -->
    <g
      v-else-if="kind === 'limiter'"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M3 6h18" stroke-dasharray="2.5 2.5" stroke-width="1.25" />
      <path d="M3 18h18" stroke-dasharray="2.5 2.5" stroke-width="1.25" />
      <path d="M3 12 7 6 9 6 13 18 15 18 19 12 21 12" />
    </g>
    <!-- HPF node: classic high-pass response curve — attenuated flat at low
         frequencies (left), rising through the cutoff, flat pass-through at
         high frequencies (right) -->
    <g
      v-else-if="kind === 'hpf'"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M3 15h5c1.5 0 2-9 3.5-9s2 9 3.5 9h6" />
    </g>
    <!-- Reverb node: concentric arcs, a signal echoing outward in a space -->
    <g
      v-else-if="kind === 'reverb'"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
    >
      <path d="M8 12a4 4 0 0 1 8 0" />
      <path d="M5 12a7 7 0 0 1 14 0" stroke-width="1.25" opacity="0.7" />
      <path d="M2 12a10 10 0 0 1 20 0" stroke-width="1" opacity="0.45" />
    </g>
    <!-- Stereo Widener node: outward-diverging arrows, L/R spreading apart -->
    <g
      v-else-if="kind === 'widener'"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path
        d="M12 4v16"
        stroke-dasharray="2 2.5"
        stroke-width="1.25"
        opacity="0.6"
      />
      <path d="M9 8 5 12l4 4" />
      <path d="M15 8l4 4-4 4" />
    </g>
    <!-- Balance/Pan node: an off-center slider on an L/R balance track -->
    <g
      v-else-if="kind === 'pan'"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
    >
      <path d="M4 12h16" />
      <circle cx="15" cy="12" r="2.75" fill="currentColor" stroke="none" />
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

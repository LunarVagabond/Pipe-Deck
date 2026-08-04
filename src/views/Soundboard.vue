<script setup lang="ts">
// Skeleton only (#396): the Soundboard tab exists and is navigable, but has
// nothing to list yet — the folder-config/file-listing backend (#394) and
// per-sound target-device mapping (#395) land in later tickets this one is
// built to receive.
</script>

<template>
  <div class="soundboard-view">
    <header class="soundboard-header view-header">
      <div>
        <p class="eyebrow">Soundboard</p>
      </div>
    </header>

    <div class="soundboard-empty-state">
      <strong>No sound clips configured yet.</strong>
      <p>
        Point Pipe Deck at a folder of sound files and assign each clip a target device to get
        started. This is coming soon.
      </p>
    </div>
  </div>
</template>

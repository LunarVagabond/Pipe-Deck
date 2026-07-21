<script setup lang="ts">
import { useNotices } from "../stores/notices";

const { notices, dismissNotice } = useNotices();
</script>

<template>
  <div v-if="notices.length" class="notice-stack" aria-live="polite">
    <article
      v-for="notice in notices"
      :key="notice.id"
      class="notice-item"
      :class="notice.kind"
    >
      <p>{{ notice.message }}</p>
      <button
        type="button"
        class="notice-dismiss"
        aria-label="Dismiss notice"
        title="Dismiss"
        @click="dismissNotice(notice.id)"
      >
        ×
      </button>
    </article>
  </div>
</template>

<script setup lang="ts">
import { useConfirm } from "../stores/confirm";

const { confirmState, answer } = useConfirm();
</script>

<template>
  <div
    v-if="confirmState.open"
    class="confirm-dialog-overlay"
    @click.self="answer(false)"
  >
    <div
      class="confirm-dialog"
      role="alertdialog"
      aria-modal="true"
      :aria-labelledby="'confirm-dialog-title'"
      :aria-describedby="'confirm-dialog-message'"
    >
      <h2 id="confirm-dialog-title">{{ confirmState.title }}</h2>
      <p id="confirm-dialog-message">{{ confirmState.message }}</p>
      <div class="confirm-dialog-actions">
        <button type="button" @click="answer(false)">
          {{ confirmState.cancelLabel }}
        </button>
        <button type="button" class="primary" @click="answer(true)">
          {{ confirmState.confirmLabel }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import ToggleSwitch from "./ToggleSwitch.vue";
import type { CapabilityInfo, PluginStatus } from "../types/graph";

const props = defineProps<{
  plugin: PluginStatus;
  capabilityMetadata: CapabilityInfo[];
  busy: boolean;
}>();

const emit = defineEmits<{
  close: [];
  "toggle-enabled": [enabled: boolean];
  "toggle-capability": [capability: string, granted: boolean];
}>();

function isEnforced(capability: string): boolean {
  return props.capabilityMetadata.find((info) => info.id === capability)?.enforced ?? false;
}
</script>

<template>
  <div class="plugin-modal" @click.self="emit('close')">
    <div class="plugin-dialog" role="dialog" aria-modal="true" :aria-labelledby="`plugin-detail-${plugin.id}`">
      <div class="plugin-dialog-header">
        <div>
          <h2 :id="`plugin-detail-${plugin.id}`">{{ plugin.name }}</h2>
          <span class="plugin-meta">v{{ plugin.version }} · {{ plugin.runtime_status }}</span>
        </div>
        <ToggleSwitch
          :model-value="plugin.enabled"
          :disabled="busy"
          @update:model-value="(enabled) => emit('toggle-enabled', enabled)"
        />
      </div>

      <p v-if="plugin.description" class="plugin-dialog-description">{{ plugin.description }}</p>

      <dl class="plugin-dialog-meta-grid">
        <div v-if="plugin.developer">
          <dt>Developer</dt>
          <dd>{{ plugin.developer }}</dd>
        </div>
        <div v-if="plugin.repo">
          <dt>Repository</dt>
          <dd><a :href="plugin.repo" target="_blank" rel="noreferrer">{{ plugin.repo }}</a></dd>
        </div>
        <div>
          <dt>Bundled</dt>
          <dd>{{ plugin.bundled ? "Yes" : "No" }}</dd>
        </div>
      </dl>

      <div v-if="plugin.requested_capabilities.length > 0" class="plugin-capabilities">
        <p class="plugin-capabilities-label">Requested capabilities</p>
        <div
          v-for="capability in plugin.requested_capabilities"
          :key="capability"
          class="settings-row plugin-capability-row"
        >
          <div>
            <p class="settings-row-label">{{ capability }}</p>
            <span v-if="!isEnforced(capability)" class="plugin-capability-badge">
              Not yet enforced
            </span>
          </div>
          <ToggleSwitch
            :model-value="plugin.granted_capabilities.includes(capability)"
            :disabled="busy || !plugin.enabled"
            :show-state-labels="false"
            @update:model-value="(granted) => emit('toggle-capability', capability, granted)"
          />
        </div>
      </div>

      <p v-if="plugin.last_error" class="settings-error plugin-dialog-error">{{ plugin.last_error }}</p>

      <div class="dialog-actions">
        <button type="button" @click="emit('close')">Close</button>
      </div>
    </div>
  </div>
</template>

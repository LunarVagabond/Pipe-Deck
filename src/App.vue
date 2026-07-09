<script setup lang="ts">
import { ref } from "vue";
import Dashboard from "./views/Dashboard.vue";

const navItems = [
  { id: "dashboard", label: "Dashboard", active: true },
  { id: "routing", label: "Routing", active: false },
  { id: "mixer", label: "Mixer", active: false },
  { id: "sources", label: "Sources", active: false },
  { id: "effects", label: "Effects", active: false },
  { id: "profiles", label: "Profiles", active: false },
  { id: "settings", label: "Settings", active: false },
];

const daemonStatus = ref("PipeWire");
</script>

<template>
  <div class="app-shell">
    <nav class="sidebar">
      <div class="brand">
        <span class="brand-icon">♪</span>
        Pipe Deck
      </div>
      <a
        v-for="item in navItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: item.active, disabled: !item.active }"
        href="#"
        @click.prevent
      >
        {{ item.label }}
      </a>
      <div class="daemon-status">
        <span class="dot" />
        {{ daemonStatus }}
      </div>
    </nav>

    <div class="main-area">
      <header class="topbar">
        <div class="topbar-title">Overview</div>
        <div class="topbar-actions">
          <button type="button" class="topbar-btn" disabled>+ New</button>
        </div>
      </header>
      <main class="content">
        <Dashboard />
      </main>
    </div>
  </div>
</template>

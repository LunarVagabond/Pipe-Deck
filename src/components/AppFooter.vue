<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "../types/app";

const appInfo = ref<AppInfo | null>(null);
const buildRevision = ref("…");

const GITHUB_PROFILE = "https://github.com/LunarVagabond";
const MIT_LICENSE_URL = "https://opensource.org/licenses/MIT";
const GITHUB_ISSUES_URL = "https://github.com/LunarVagabond/Pipe-Deck/issues/new?template=bug_report.yml";

async function openExternal(event: MouseEvent, url: string) {
  event.preventDefault();
  try {
    await invoke("open_url", { url });
  } catch (error) {
    console.error("Failed to open URL:", error);
  }
}

onMounted(async () => {
  try {
    appInfo.value = await invoke<AppInfo>("get_app_info");
    buildRevision.value = appInfo.value.buildRevision;
  } catch {
    appInfo.value = null;
  }
});
</script>

<template>
  <footer class="app-footer">
    <div class="app-footer-left">
      <a
        class="app-footer-link"
        :href="GITHUB_PROFILE"
        @click="openExternal($event, GITHUB_PROFILE)"
      >
        @LunarVagabond
      </a>
      <span class="app-footer-sep">-</span>
      <a
        class="app-footer-link"
        :href="MIT_LICENSE_URL"
        @click="openExternal($event, MIT_LICENSE_URL)"
      >
        MIT License
      </a>
    </div>

    <div class="app-footer-center">
      <a
        class="app-footer-link app-footer-bug-link"
        :href="GITHUB_ISSUES_URL"
        @click="openExternal($event, GITHUB_ISSUES_URL)"
      >
        Found a Bug? Report it!
      </a>
    </div>

    <div class="app-footer-right">
      <span class="app-footer-revision">{{ buildRevision }}</span>
    </div>
  </footer>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "../types/app";

const appInfo = ref<AppInfo | null>(null);
const buildRevision = ref("…");

const GITHUB_PROFILE = "https://github.com/LunarVagabond";
const BMC_URL = "https://www.buymeacoffee.com/lunarvagabond";
const BMC_BUTTON_SRC = "https://cdn.buymeacoffee.com/buttons/v2/default-violet.png";
const MIT_LICENSE_URL = "https://opensource.org/licenses/MIT";

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
      <span class="app-footer-revision">{{ buildRevision }}</span>
    </div>

    <div class="app-footer-support">
      <span class="app-footer-support-text">Enjoying Pipe Deck? Consider</span>
      <a
        class="app-footer-bmc"
        :href="BMC_URL"
        aria-label="Buy me a coffee"
        @click="openExternal($event, BMC_URL)"
      >
        <img :src="BMC_BUTTON_SRC" alt="Buy me a coffee" width="162" height="45" />
      </a>
    </div>
  </footer>
</template>

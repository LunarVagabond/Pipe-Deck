<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "../types/app";

const appInfo = ref<AppInfo | null>(null);
const buildRevision = ref("…");

const GITHUB_PROFILE = "https://github.com/LunarVagabond";
const MIT_LICENSE_URL = "https://opensource.org/licenses/MIT";
const GITHUB_ISSUES_URL = "https://github.com/LunarVagabond/Pipe-Deck/issues/new?template=bug_report.yml";
const GITHUB_REPO = "https://github.com/LunarVagabond/Pipe-Deck";

// `buildRevision` is either the exact git tag a release was built from (e.g.
// `v0.0.2-alpha`) or a short commit hash for dev/CI builds — `releaseVersion`
// is only ever set when it's the former (see `release_version_from_revision`
// in app_info.rs), so its presence is what distinguishes the two cases.
const revisionUrl = computed(() => {
  if (!appInfo.value || buildRevision.value === "unknown") return null;
  return appInfo.value.releaseVersion
    ? `${GITHUB_REPO}/releases/tag/${buildRevision.value}`
    : `${GITHUB_REPO}/commit/${buildRevision.value}`;
});

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
      <span v-if="appInfo?.pipewireVersion" class="app-footer-pipewire-version">
        PipeWire {{ appInfo.pipewireVersion }}
      </span>
      <a
        v-if="revisionUrl"
        class="app-footer-revision"
        :href="revisionUrl"
        @click="openExternal($event, revisionUrl)"
      >
        {{ buildRevision }}
      </a>
      <span v-else class="app-footer-revision">{{ buildRevision }}</span>
    </div>
  </footer>
</template>

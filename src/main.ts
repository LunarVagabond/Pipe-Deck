import { createApp } from "vue";
import App from "./App.vue";
import "@vue-flow/core/dist/style.css";
import "@vue-flow/core/dist/theme-default.css";
import "@vue-flow/controls/dist/style.css";
import "./styles/main.scss";
import { useTheme } from "./stores/theme";
import { useNoticeSettings } from "./stores/notices";

async function bootstrap() {
  await useTheme().initTheme();
  // Unlike theme (which must apply before first paint to avoid a flash of
  // the wrong colors), the notice duration has a fine default and isn't
  // visible until a notice actually fires — no reason to make mount wait on
  // a second sequential IPC round trip for it.
  void useNoticeSettings().initNoticeSettings();
  createApp(App).mount("#app");
}

void bootstrap();

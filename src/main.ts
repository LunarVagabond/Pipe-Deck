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
  await useNoticeSettings().initNoticeSettings();
  createApp(App).mount("#app");
}

void bootstrap();

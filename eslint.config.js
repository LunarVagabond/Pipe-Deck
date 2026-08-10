// ESLint flat config for the Vue 3 + TypeScript frontend (src/).
//
// Kept deliberately narrow and correctness-focused for now (issue #440):
// unused vars/imports, Vue-specific footguns (missing :key in v-for, etc).
// Formatting/stylistic rules (indentation, quotes, semicolons, ...) are
// intentionally left out — that's Prettier's job (issue #470), and
// overlapping rulesets just make the two tools fight each other.
import js from "@eslint/js";
import pluginVue from "eslint-plugin-vue";
import { defineConfigWithVueTs, vueTsConfigs } from "@vue/eslint-config-typescript";
import globals from "globals";

export default defineConfigWithVueTs(
  {
    name: "app/files-to-lint",
    files: ["src/**/*.{ts,tsx,vue}"],
  },
  {
    name: "app/ignores",
    // src/vite-env.d.ts is Vite/Vue's own generated ambient-module
    // boilerplate (create-vue scaffolding), not app code.
    ignores: ["src/e2e/**", "src/vite-env.d.ts", "**/dist/**", "**/node_modules/**"],
  },

  js.configs.recommended,
  pluginVue.configs["flat/recommended"],
  vueTsConfigs.recommended,

  {
    name: "app/languageOptions",
    languageOptions: {
      globals: {
        ...globals.browser,
      },
    },
  },

  {
    name: "app/rules",
    rules: {
      // Formatting is Prettier's job (#470) — don't fight it here.
      "vue/html-indent": "off",
      "vue/max-attributes-per-line": "off",
      "vue/singleline-html-element-content-newline": "off",
      "vue/multiline-html-element-content-newline": "off",
      "vue/html-self-closing": "off",
      "vue/html-closing-bracket-newline": "off",
      "vue/attributes-order": "off",
      "vue/first-attribute-linebreak": "off",

      // This codebase names view/page components after their sidebar
      // destination (Dashboard.vue, Mixer.vue, Routing.vue, ...) — see
      // src/views in CLAUDE.md. That's an established, deliberate
      // convention, not a bug, so this naming-only rule isn't useful here.
      "vue/multi-word-component-names": "off",

      // Leading-underscore params/vars are this codebase's convention for
      // "intentionally unused" (e.g. signature-compatible callback params).
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
);

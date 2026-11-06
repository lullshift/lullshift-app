import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      ".claude/**",
      "**/node_modules/**",
      "**/dist/**",
      "**/target/**",
      "**/src/generated/**",
      "apps/desktop/src-tauri/**",
      "docs/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{js,mjs}"],
    languageOptions: { globals: globals.node },
  },
  {
    // Activities are classic browser scripts in a sandboxed frame (ADR-012); the bridge client
    // and shared logic files are loaded first and become page globals. Kit files
    // (lullshift-<kit>-kit-v1.js, ADR-019) add LullshiftKits.
    files: [
      "activities/**/*.js",
      "packages/activity-sdk/bridge/*.js",
      "tools/lullshift-cli/templates/**/*.js",
    ],
    languageOptions: {
      sourceType: "script",
      globals: {
        ...globals.browser,
        LullshiftBridge: "readonly",
        LullshiftKits: "readonly",
        LanternLogic: "readonly",
      },
    },
    rules: { "@typescript-eslint/no-unused-vars": ["error", { caughtErrors: "none" }] },
  },
  {
    files: ["tools/lullshift-cli/harness/**/*.ts"],
    languageOptions: { globals: globals.browser },
  },
  {
    files: ["apps/desktop/src/**/*.{ts,tsx}"],
    languageOptions: { globals: globals.browser },
    plugins: { "react-hooks": reactHooks },
    rules: reactHooks.configs.recommended.rules,
  },
);

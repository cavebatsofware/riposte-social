const js = require("@eslint/js");
const tseslint = require("typescript-eslint");
const a11yPlugin = require("@barrierbreak/eslint-plugin-a11yinspect");
const reactPlugin = require("eslint-plugin-react");
const reactHooks = require("eslint-plugin-react-hooks");
const globals = require("globals");

// admin-frontend is intentionally out of scope for now; lint targets the
// social-frontend only. The frontend is TypeScript only (no .js/.jsx).
const frontendTs = ["social-frontend/**/*.ts"];
const frontendTsx = ["social-frontend/**/*.tsx"];

const sharedGlobals = {
  ...globals.browser,
  ...globals.es2024,
};

// Flatten a config array (e.g. tseslint.configs.recommended) into a single
// rules object, dropping non-rule fields that would conflict when spread.
const flattenRules = (configOrArray) => {
  const arr = Array.isArray(configOrArray) ? configOrArray : [configOrArray];
  return Object.assign({}, ...arr.map((c) => c.rules || {}));
};

const tsRecommendedRules = flattenRules(tseslint.configs.recommended);
const reactRecommendedRules = flattenRules(reactPlugin.configs.flat.recommended);
const reactJsxRuntimeRules = flattenRules(reactPlugin.configs.flat["jsx-runtime"]);
const reactHooksRecommendedRules = flattenRules(reactHooks.configs.recommended);

// a11yinspect rules the plugin enables that don't fit our SPA / component
// model. The axe Cypress smoke (`bun run a11y:smoke`) catches the real
// WCAG violations at runtime, so we keep ESLint focused on static-attribute
// checks where it has signal and disable the rules whose AST checks can't
// see through JSX expressions.
const a11yRuleOverrides = {
  // Page-document checks fire once per module; only meaningful on the HTML
  // shell, which is exercised by axe in the a11y smoke.
  "a11yinspect/title-element-error": "off",
  "a11yinspect/landmark-element-error": "off",
  "a11yinspect/skip-link-warning": "off",
  // <section> is only a landmark when accessibly named. The plugin flags
  // every <section>; that's noise on layout groupings.
  "a11yinspect/section-element-warning": "off",
  // "Multiple h1 elements" false-positives on conditional renders (e.g.
  // step-based screens) where only one branch mounts at a time.
  "a11yinspect/heading-element-warning": "off",
  // The "needs accessible text content" family can't see through JSX
  // expressions, so `<h1>{t("title")}</h1>`, `<label>{t("...")}</label>`,
  // `<button>{label}</button>`, and `<a>{t("...")}</a>` all read as empty.
  // With i18n across the app this is endemic noise. axe handles these.
  "a11yinspect/heading-element-error": "off",
  "a11yinspect/label-element-error": "off",
  "a11yinspect/button-element-error": "off",
  "a11yinspect/a-element-error": "off",
  "a11yinspect/menu-element-error": "off",
  // Flags `alt=""` as "empty alt" even though null alt is the correct
  // WAI-ARIA pattern for decorative images.
  "a11yinspect/img-element-error": "off",
};

module.exports = [
  {
    ignores: [
      "node_modules/",
      "admin-assets/",
      "social-assets/",
      "target/",
      "scripts/",
      "eslint.config.js",
    ],
  },
  js.configs.recommended,
  // Plain TS (no JSX). TypeScript already checks names and DOM lib globals,
  // so eslint's no-undef would only false-positive here.
  {
    files: frontendTs,
    languageOptions: {
      parser: tseslint.parser,
      ecmaVersion: "latest",
      sourceType: "module",
      globals: sharedGlobals,
    },
    plugins: {
      "@typescript-eslint": tseslint.plugin,
    },
    rules: {
      ...tsRecommendedRules,
      "no-undef": "off",
      "no-unused-vars": "off",
      "@typescript-eslint/no-unused-vars": ["warn", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
    },
  },
  // TSX (React + a11y rules apply).
  {
    files: frontendTsx,
    languageOptions: {
      parser: tseslint.parser,
      ecmaVersion: "latest",
      sourceType: "module",
      parserOptions: { ecmaFeatures: { jsx: true } },
      globals: sharedGlobals,
    },
    plugins: {
      a11yinspect: a11yPlugin,
      react: reactPlugin,
      "react-hooks": reactHooks,
      "@typescript-eslint": tseslint.plugin,
    },
    settings: { react: { version: "detect" } },
    rules: {
      ...tsRecommendedRules,
      ...reactRecommendedRules,
      ...reactJsxRuntimeRules,
      ...reactHooksRecommendedRules,
      ...a11yPlugin.configs.recommended.rules,
      ...a11yRuleOverrides,
      // TypeScript covers prop validation; eslint-plugin-react's prop-types
      // rule is redundant noise on a TS codebase.
      "react/prop-types": "off",
      "no-undef": "off",
      "no-unused-vars": "off",
      "@typescript-eslint/no-unused-vars": ["warn", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
    },
  },
];

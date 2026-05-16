const js = require("@eslint/js");
const tseslint = require("typescript-eslint");
const a11yPlugin = require("@barrierbreak/eslint-plugin-a11yinspect");
const reactPlugin = require("eslint-plugin-react");
const reactHooks = require("eslint-plugin-react-hooks");
const globals = require("globals");

const frontends = [
  "social-frontend/**/*.{js,jsx}",
  "admin-frontend/**/*.{js,jsx}",
];

const frontendsTsx = [
  "social-frontend/**/*.{ts,tsx}",
  "admin-frontend/**/*.{ts,tsx}",
];

const sharedPlugins = {
  a11yinspect: a11yPlugin,
  react: reactPlugin,
  "react-hooks": reactHooks,
};

const sharedGlobals = {
  ...globals.browser,
  ...globals.es2024,
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
  // JS/JSX: both frontends
  {
    files: frontends,
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      parserOptions: { ecmaFeatures: { jsx: true } },
      globals: sharedGlobals,
    },
    plugins: sharedPlugins,
    settings: { react: { version: "detect" } },
    rules: {
      ...a11yPlugin.configs.recommended.rules,
      "react/jsx-uses-vars": "error",
      "no-unused-vars": ["warn", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
    },
  },
  // TS/TSX: both frontends
  {
    files: frontendsTsx,
    languageOptions: {
      parser: tseslint.parser,
      ecmaVersion: "latest",
      sourceType: "module",
      parserOptions: { ecmaFeatures: { jsx: true } },
      globals: sharedGlobals,
    },
    plugins: {
      ...sharedPlugins,
      "@typescript-eslint": tseslint.plugin,
    },
    settings: { react: { version: "detect" } },
    rules: {
      ...a11yPlugin.configs.recommended.rules,
      "react/jsx-uses-vars": "error",
      "no-unused-vars": "off",
      "@typescript-eslint/no-unused-vars": ["warn", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
    },
  },
];

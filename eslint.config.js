// ESLint flat config. Scoped to the social-frontend with the
// `eslint-plugin-jsx-a11y` recommended ruleset. Admin-frontend is
// intentionally excluded; add it to the `files` glob to bring it under
// the same lint surface.

const js = require("@eslint/js");
const jsxA11y = require("eslint-plugin-jsx-a11y");
const reactPlugin = require("eslint-plugin-react");
const reactHooks = require("eslint-plugin-react-hooks");
const globals = require("globals");

module.exports = [
  {
    ignores: [
      "node_modules/",
      "admin-assets/",
      "social-assets/",
      "target/",
      "scripts/",
      "admin-frontend/",
      "vite.config.js",
      "vite.social.config.js",
      "eslint.config.js",
    ],
  },
  js.configs.recommended,
  {
    files: ["social-frontend/**/*.{js,jsx}"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      parserOptions: {
        ecmaFeatures: { jsx: true },
      },
      globals: {
        ...globals.browser,
        ...globals.es2024,
      },
    },
    plugins: {
      "jsx-a11y": jsxA11y,
      react: reactPlugin,
      "react-hooks": reactHooks,
    },
    settings: {
      react: { version: "detect" },
    },
    rules: {
      // jsx-a11y recommended set
      ...jsxA11y.configs.recommended.rules,
      // Most React rules duplicate what TypeScript / careful review
      // already catch, but a few are ARIA-adjacent. Keep the set
      // minimal so the lint signal stays a11y-focused.
      "react/jsx-uses-vars": "error",
      // The base ESLint rules surface a few real bugs (no-unused-vars,
      // no-undef). Tune as we hit noise.
      "no-unused-vars": ["warn", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
    },
  },
];

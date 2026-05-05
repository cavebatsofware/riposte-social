// Cypress config. Today this serves a single purpose: hosting the
// `axe-core` smoke harness. The dev
// server has to be running on baseUrl before invoking cypress.
//
// Future test specs can land alongside the a11y smoke; until then
// the spec pattern is intentionally narrow.

const { defineConfig } = require("cypress");

module.exports = defineConfig({
  e2e: {
    baseUrl: process.env.CYPRESS_BASE_URL || "http://localhost:3000",
    supportFile: "cypress/support/e2e.js",
    specPattern: "cypress/e2e/**/*.cy.{js,jsx}",
    fixturesFolder: false,
    video: false,
    screenshotOnRunFailure: false,
    env: {
      A11Y_STRICTNESS: process.env.A11Y_STRICTNESS,
    },
    setupNodeEvents() {
      // No plugins in use today.
    },
  },
});

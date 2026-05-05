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
      // Mirror the test-admin credentials from the env into Cypress
      // so `cy.login()` can read them via `Cypress.env(...)`. Defaults
      // match `docker-compose.test.yml`'s defaults.
      TEST_ADMIN_EMAIL:
        process.env.TEST_ADMIN_EMAIL || "admin@test.local",
      TEST_ADMIN_PASSWORD:
        process.env.TEST_ADMIN_PASSWORD || "test_admin_password",
    },
    setupNodeEvents(on) {
      // Surface `cy.task("log", ...)` to the runner output so violation
      // detail from the a11y smoke shows up alongside the test failure.
      on("task", {
        log(message) {
          // eslint-disable-next-line no-console
          console.log(message);
          return null;
        },
      });
    },
  },
});

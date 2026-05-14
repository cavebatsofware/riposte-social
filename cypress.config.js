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
    // Failure screenshots: opt-in per run with `CYPRESS_SCREENSHOTS=true`.
    // Only fires on a failing assertion, so when enabled the default
    // success path stays cheap; turning it on for every run still
    // changes config in ways CI sometimes complains about, so leave the
    // default off and flip it deliberately when you're investigating.
    screenshotOnRunFailure: process.env.CYPRESS_SCREENSHOTS === "true",
    // Video: opt-in with `CYPRESS_VIDEO=true`. Per-spec mp4 capture is
    // heavy enough that it stays off by default; enable when you want
    // a recording you can scrub through.
    video: process.env.CYPRESS_VIDEO === "true",
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

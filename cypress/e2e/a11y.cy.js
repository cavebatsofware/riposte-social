// axe-core smoke harness. Visits each major public route, injects
// axe, and asserts there are no axe violations at the configured
// impact levels.
//
// Run locally:
//   1. start the social-frontend dev server: `npm run dev:social`
//   2. in another shell: `npm run a11y:smoke` (default, gate level)
//      or `npm run a11y:smoke:strict` (every impact level surfaced)
//
// Strictness is controlled by the `A11Y_STRICTNESS` Cypress env var,
// pulled from `process.env.A11Y_STRICTNESS` in cypress.config.js so
// `npm run a11y:smoke` and `npm run a11y:smoke:strict` work without
// extra wiring:
//   - "ci" (default): serious + critical. The gate level CI runs.
//   - "strict": minor + moderate + serious + critical. Surfaces every
//     violation axe knows about. Useful during development to see the
//     full picture before deciding what to fix vs document.
//
// Anonymous public routes are the only ones smoke-tested today
// because they don't require seed data or auth state. Add routes here
// as their landmark and ARIA work lands.

const PUBLIC_ROUTES = [
  { path: "/", label: "Feed (anonymous)" },
  { path: "/login", label: "Login" },
  { path: "/articles", label: "Articles browse (anonymous)" },
];

const STRICTNESS_LEVELS = {
  ci: ["serious", "critical"],
  strict: ["minor", "moderate", "serious", "critical"],
};

const STRICTNESS = Cypress.env("A11Y_STRICTNESS") || "ci";
const FAIL_ON_IMPACTS = STRICTNESS_LEVELS[STRICTNESS] || STRICTNESS_LEVELS.ci;

// Log every violation to the test output so failures surface the rule
// id, impact, target selector, and help text. Without this the runner
// only reports "N violations" and you have to re-run with debug to
// learn what's wrong.
function logViolations(violations) {
  if (!violations.length) return;
  cy.task("log", `\n  axe violations (${violations.length}):`);
  violations.forEach((v) => {
    cy.task(
      "log",
      `    [${v.impact}] ${v.id}: ${v.help} (${v.nodes.length} node${v.nodes.length === 1 ? "" : "s"})`,
    );
    v.nodes.forEach((n) => {
      cy.task("log", `      ${n.target.join(" ")}`);
    });
  });
}

describe(`a11y smoke (${STRICTNESS})`, () => {
  PUBLIC_ROUTES.forEach(({ path, label }) => {
    it(`${label}: no axe violations at impact ${FAIL_ON_IMPACTS.join("|")}`, () => {
      cy.visit(path);
      // Wait for the SPA shell to mount before scanning. cy.visit only
      // resolves on `load`; React renders on the next frame after the
      // bundle parses, and axe-core needs the rendered DOM to evaluate
      // landmark and heading rules correctly.
      cy.get("main#main-content", { timeout: 10000 }).should("exist");
      cy.injectAxe();
      cy.checkA11y(
        null,
        { includedImpacts: FAIL_ON_IMPACTS },
        logViolations,
      );
    });
  });
});

// Cypress support file. Loaded before every spec.
// `cypress-axe` adds `cy.injectAxe()` and `cy.checkA11y()` commands
// that the a11y smoke spec relies on. `auth.js` adds `cy.login()`
// for authed-route specs that want to skip the UI sign-in flow.

import "cypress-axe";
import "./auth.js";

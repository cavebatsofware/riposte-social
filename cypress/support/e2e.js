// Cypress support file. Loaded before every spec.
// `cypress-axe` adds `cy.injectAxe()` and `cy.checkA11y()` commands
// that the a11y smoke spec relies on. `auth.js` adds `cy.login()`
// for authed-route specs that want to skip the UI sign-in flow.

import "cypress-axe";
import "./auth.js";

// Pre-acknowledge the cookie consent banner before any spec mounts
// the SPA. Otherwise the banner overlays the bottom of the viewport
// and covers form-action buttons (Save / Submit), which Cypress's
// click actionability check refuses to interact with. The banner's
// own `STORAGE_KEY` constant is `rs_cookie_ack_v1` (see
// social-frontend/src/components/CookieBanner.jsx).
Cypress.on("window:before:load", (win) => {
  win.localStorage.setItem("rs_cookie_ack_v1", "1");
});

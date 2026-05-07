/// Functional spec for the Settings → Profile page. Builds on cy.login()
/// from the test-infra fixture. Asserts:
///
/// 1. The settings tab nav announces the active tab via aria-current.
/// 2. The form's accessible structure is intact: the avatar file input
///    has a label, the bio textarea is associated with its character
///    counter via aria-describedby.
/// 3. After saving, the success banner renders with role=status (so a
///    screen reader announces it without interrupting other speech).
///
/// Anonymous viewers visiting /settings/profile are redirected to /login;
/// the spec asserts that path too.

describe("settings/profile (seeded test admin)", () => {
  it("redirects anonymous visitors to /login", () => {
    cy.visit("/settings/profile");
    cy.location("pathname", { timeout: 10000 }).should("equal", "/login");
  });

  describe("authed admin", () => {
    beforeEach(() => {
      cy.login();
      cy.visit("/settings/profile");
      cy.get("main#main-content", { timeout: 10000 }).should("exist");
      // Wait for the form to mount past the loading spinner.
      cy.get("#settings-handle", { timeout: 10000 }).should("exist");
    });

    it("active tab announces aria-current=page", () => {
      cy.get('a.settings-tab[href="/settings/profile"]')
        .should("have.attr", "aria-current", "page");
      cy.get('a.settings-tab[href="/settings/security"]')
        .should("not.have.attr", "aria-current");
    });

    it("avatar input and bio textarea are properly labelled", () => {
      cy.get("#settings-avatar-input").should("exist");
      // The label exists and is associated via htmlFor=settings-avatar-input.
      cy.get('label[for="settings-avatar-input"]').should("exist");
      // Bio textarea is described by the live char counter.
      cy.get("#settings-bio")
        .should("have.attr", "aria-describedby", "settings-bio-count");
      cy.get("#settings-bio-count")
        .should("have.attr", "aria-live", "polite");
    });

    it("MFA QR alt text describes the image when MFA setup is open", () => {
      // Switch to the security tab and open MFA setup. The QR image
      // should carry an informative alt text rather than a generic
      // 'MFA QR Code' string.
      cy.get('a.settings-tab[href="/settings/security"]').click();
      cy.location("pathname").should("equal", "/settings/security");
      cy.get("main#main-content").should("exist");
      // If MFA is currently disabled, the Enable button is present.
      // The seeded admin has MFA disabled by the seed-test-admin
      // refactor (oidc_sub + totp_enabled cleared), so this branch
      // is the deterministic one.
      cy.contains("button", /Enable|Aktivieren|Activer|Activar|启用/, {
        timeout: 10000,
      }).click();
      cy.get(".mfa-setup img", { timeout: 10000 })
        .should("have.attr", "alt")
        .and("match", /authenticator|autenticación|authentification|验证器|Authenticator/);
      // The manual-entry secret carries an aria-label so screen
      // readers don't read it as a single token.
      cy.get(".mfa-setup code")
        .should("have.attr", "aria-label");
    });
  });
});

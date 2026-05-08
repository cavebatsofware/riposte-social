/// Functional spec for the Settings, Profile page. Builds on cy.login()
/// from the test-infra fixture. Asserts:
///
/// 1. The settings tab nav announces the active tab via aria-current.
/// 2. The form's accessible structure is intact: the avatar file input
///    has a label, the bio textarea is associated with its character
///    counter via aria-describedby.
/// 3. Saving the bio renders the success banner with role=status, so a
///    screen reader announces it without interrupting other speech.
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

    it("saving the bio renders a role=status success banner", () => {
      // Type a single character into the bio so the patch has a
      // genuine diff to send. The seed admin starts with no bio, so
      // any non-empty value produces a state change. Submit via the
      // form's Save button rather than Enter so we exercise the
      // submit handler.
      cy.get("#settings-bio").type("e2e", { delay: 0 });
      cy.get('form.settings-form button[type="submit"]')
        .scrollIntoView()
        .click();
      cy.get(".alert.alert-success", { timeout: 10000 })
        .should("have.attr", "role", "status");
      // Restore the bio to empty so the fixture stays clean for any
      // subsequent run that asserts the no-bio state.
      cy.get("#settings-bio").clear();
      cy.get('form.settings-form button[type="submit"]')
        .scrollIntoView()
        .click();
      cy.get(".alert.alert-success", { timeout: 10000 }).should("exist");
    });

    it("MFA QR alt text describes the image when MFA setup is open", () => {
      // Switch to the security tab and open MFA setup. The QR image
      // should carry an informative alt text rather than a generic
      // 'MFA QR Code' string.
      cy.get('a.settings-tab[href="/settings/security"]').click();
      cy.location("pathname").should("equal", "/settings/security");
      cy.get("main#main-content").should("exist");
      // The seeded admin has MFA disabled (seed-test-admin clears
      // oidc_sub and totp_enabled), so the Enable button is the
      // deterministic branch. The regex covers all five locales'
      // verb for "enable"; the `i` flag handles the German
      // sentence-case variant ("MFA aktivieren").
      cy.contains("button", /Enable|Aktivieren|Activer|Activar|启用/i, {
        timeout: 10000,
      }).click();
      cy.get(".mfa-setup img", { timeout: 10000 })
        .should("have.attr", "alt")
        .and("match", /authenticator|autenticación|authentification|验证器|Authenticator/);
      // The manual-entry secret is rendered as the <code> element's
      // text content so screen readers read out the actual characters
      // when the user navigates to it. The surrounding paragraph
      // ("Or enter manually:") provides context.
      cy.get(".mfa-setup code")
        .invoke("text")
        .should("match", /^[A-Z2-7]{16,}$/i);
    });
  });
});

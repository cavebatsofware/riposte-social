/// First real functional spec, using the seeded admin from
/// docker-compose.test.yml's app-test container. Verifies the infra
/// delivers what cy.login() promises end-to-end:
///
/// 1. Anonymous viewer at `/` sees a Sign in link in the header.
/// 2. After `cy.login()`, the same `/` shows the avatar / user-menu
///    trigger instead of the Sign in link, proving the session
///    cookie persists across `cy.visit`.
/// 3. The authed viewer can reach `/compose`, which is gated to
///    admin/poster, and the body textarea + Visibility legend
///    render. (The non-admin commenter role would be redirected to
///    `/` by the page-level role gate.)
///
/// Spec is intentionally narrow: it asserts the auth-state change
/// surfaces and the page-level role gate, not deeper Compose
/// behavior. Future phase PRs add their own functional specs that
/// piggyback on the same cy.login() fixture.

describe("auth flow (seeded test admin)", () => {
  it("anonymous header shows the Sign in link", () => {
    cy.visit("/");
    cy.get("main#main-content", { timeout: 10000 }).should("exist");
    cy.get(".layout-auth-btn").should("contain.text", "Sign in");
    cy.get(".user-menu-trigger").should("not.exist");
  });

  describe("authed admin", () => {
    beforeEach(() => {
      // Login before any page load so cy.request() sets the session
      // cookie on a clean browser jar (no prior anonymous cookie to
      // conflict with). Pattern mirrors settings_profile.cy.js.
      cy.login().then((user) => {
        expect(user.role).to.equal("administrator");
        expect(user.email_verified).to.be.true;
        // Handle is derived from TEST_ADMIN_EMAIL's local-part; assert
        // non-empty so the spec works with any seeded fixture.
        expect(user.handle).to.be.a("string").and.not.be.empty;
      });
      cy.visit("/");
      cy.get("main#main-content", { timeout: 10000 }).should("exist");
    });

    it("the same route shows the user menu after login", () => {
      cy.get(".user-menu-trigger").should("exist");
      cy.get(".layout-auth-btn").should("not.exist");
    });

    it("authed admin can reach /compose with the body textarea and visibility legend", () => {
      cy.visit("/compose");
      cy.get("#compose-body", { timeout: 10000 }).should("exist");
      cy.get(".compose-card").should("exist");
      cy.contains("Visibility").should("exist");
    });
  });
});

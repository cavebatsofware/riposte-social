/// Functional spec for the password-mode Sign-in page. Builds on the
/// shared `cy.login()` fixture to drive the success path through the
/// real `/api/auth/login` endpoint. The test app's auth-config is
/// password-mode (OIDC disabled), so this spec exercises the form
/// branch of `Login.jsx`. Asserts:
///
/// 1. The form renders with labelled email + password inputs, a
///    submit button, and the invite-only notice.
/// 2. The email input receives focus on mount via the ref+useEffect
///    pattern (replaces the `autoFocus` prop the lint baseline forbids).
/// 3. The form's aria-busy attribute starts false and the inputs have
///    the right autoComplete hints for password managers.
/// 4. Submitting bad credentials renders an inline error with
///    role="alert" so screen readers announce it with urgency.
/// 5. Submitting the seeded admin's credentials lands the viewer at
///    `/` with the user-menu trigger visible (proving the session
///    cookie persists post-redirect).

describe("auth/login (password mode)", () => {
  beforeEach(() => {
    cy.visit("/login");
    cy.get("main#main-content", { timeout: 10000 }).should("exist");
    cy.get("#login-email", { timeout: 10000 }).should("exist");
  });

  it("renders the labelled form and invite-only notice", () => {
    cy.get('label[for="login-email"]').should("exist");
    cy.get('label[for="login-password"]').should("exist");
    cy.get("#login-email")
      .should("have.attr", "type", "email")
      .and("have.attr", "autocomplete", "email")
      .and("have.attr", "required");
    cy.get("#login-password")
      .should("have.attr", "type", "password")
      .and("have.attr", "autocomplete", "current-password")
      .and("have.attr", "required");
    cy.get('button[type="submit"].btn-primary').should("exist");
    cy.get(".auth-invite-notice").should("exist");
  });

  it("moves focus into the email field on mount", () => {
    cy.focused().should("have.attr", "id", "login-email");
  });

  it("the form is not aria-busy on first paint", () => {
    cy.get("form.auth-form").should("have.attr", "aria-busy", "false");
  });

  it("bad credentials render a role=alert error", () => {
    cy.get("#login-email").type("nobody@test.local", { delay: 0 });
    cy.get("#login-password").type("wrong_password_value", { delay: 0 });
    cy.get('form.auth-form button[type="submit"]')
      .scrollIntoView()
      .click();
    cy.get(".alert.alert-error", { timeout: 10000 })
      .should("have.attr", "role", "alert");
    // We stayed on /login; the form did not redirect.
    cy.location("pathname").should("equal", "/login");
  });

  it("seeded admin credentials redirect to / and surface the user menu", () => {
    const email = Cypress.env("TEST_ADMIN_EMAIL") || "admin@test.local";
    const password =
      Cypress.env("TEST_ADMIN_PASSWORD") || "test_admin_password";
    cy.get("#login-email").type(email, { delay: 0 });
    cy.get("#login-password").type(password, { delay: 0 });
    cy.get('form.auth-form button[type="submit"]')
      .scrollIntoView()
      .click();
    cy.location("pathname", { timeout: 10000 }).should("equal", "/");
    cy.get(".user-menu-trigger", { timeout: 10000 }).should("exist");
  });
});

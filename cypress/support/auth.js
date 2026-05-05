/// `cy.login(email, password)` authenticates against the test app's
/// `/api/auth/login` endpoint and persists the session cookie for the
/// rest of the spec. Both arguments are optional and default to the
/// seeded test admin credentials baked into `docker-compose.test.yml`.
///
/// `/api/auth/login` is CSRF-protected, so this command first GETs
/// `/api/auth/csrf-token` (which establishes a session cookie and
/// returns a token) and then POSTs the login with the token in the
/// `x-csrf-token` header. Both requests share the session cookie that
/// Cypress retains automatically.
///
/// Returns the parsed login response (`UserResponse`) so callers can
/// assert role / handle / id.
Cypress.Commands.add("login", (email, password) => {
  const user =
    email || Cypress.env("TEST_ADMIN_EMAIL") || "admin@test.local";
  const pass =
    password ||
    Cypress.env("TEST_ADMIN_PASSWORD") ||
    "test_admin_password";
  return cy
    .request({
      method: "GET",
      url: "/api/auth/csrf-token",
    })
    .then((csrf) =>
      cy.request({
        method: "POST",
        url: "/api/auth/login",
        headers: { "x-csrf-token": csrf.body.token },
        body: { email: user, password: pass },
        failOnStatusCode: true,
      }),
    )
    .then((response) => response.body);
});

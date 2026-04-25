/**
 * Fetch wrapper that automatically includes CSRF token from session.
 *
 * TODO(Phase 1): csrf-token endpoint moves from /api/admin/csrf-token to
 * /api/csrf-token when OIDC/auth routes are unified under /api/auth/*.
 */

let csrfToken = null;

async function ensureCsrfToken() {
  if (csrfToken) return csrfToken;

  const response = await fetch("/api/admin/csrf-token", {
    credentials: "include",
  });

  if (response.ok) {
    const data = await response.json();
    csrfToken = data.token;
  }

  return csrfToken;
}

export function clearCsrfToken() {
  csrfToken = null;
}

export async function fetchApi(url, options = {}) {
  const headers = { ...options.headers };

  if (["POST", "PUT", "DELETE", "PATCH"].includes(options.method?.toUpperCase())) {
    const token = await ensureCsrfToken();
    if (token) {
      headers["x-csrf-token"] = token;
    }
  }

  return fetch(url, {
    ...options,
    headers,
    credentials: "include",
  });
}

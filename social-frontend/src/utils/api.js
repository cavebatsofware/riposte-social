/**
 * Fetch wrapper that automatically includes CSRF token from session.
 */

let csrfToken = null;

async function ensureCsrfToken() {
  if (csrfToken) return csrfToken;

  const response = await fetch("/api/auth/csrf-token", {
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

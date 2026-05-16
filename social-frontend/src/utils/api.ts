/**
 * Fetch wrapper that automatically includes CSRF token from session.
 */

let csrfToken: string | null = null;

async function ensureCsrfToken(): Promise<string | null> {
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

export function clearCsrfToken(): void {
  csrfToken = null;
}

export async function fetchApi(url: string, options: RequestInit = {}): Promise<Response> {
  const headers: Record<string, string> = {
    ...(options.headers as Record<string, string> | undefined),
  };

  if (["POST", "PUT", "DELETE", "PATCH"].includes((options.method ?? "").toUpperCase())) {
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

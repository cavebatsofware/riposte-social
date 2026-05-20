/**
 * Fetch wrapper that automatically includes CSRF token from session.
 *
 * Reserves a unit in the global loading bucket (see `loadingState`) for the
 * duration of the round trip. Critically, "duration" here means until the
 * caller has read the response body, not until headers arrive. `fetch`
 * resolves at headers, so the body may still be streaming for hundreds of
 * milliseconds (or seconds, for a 1.5MB batch) afterwards. To match the
 * Network tab's "in flight" window, the returned Response is wrapped so
 * that its body-consumer methods (`json` / `text` / `blob` / `arrayBuffer`
 * / `formData`) fire `endLoad` after their promise settles.
 *
 * Failure modes that close the unit at headers instead of body-read:
 *   - `fetch` itself throws (network error or abort).
 *   - The response is non-OK (4xx/5xx); the caller may inspect `response.ok`
 *     and discard without reading the body, which would otherwise leak a
 *     unit forever.
 */

import { endLoad, startLoad, type LoadOptions } from "./loadingState";

export interface RiposteRequestInit extends RequestInit {
  /** `[current, total]` for serial loop progress. See `loadingState`. */
  hasProgress?: [number, number];
}

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

type BodyMethod = "json" | "text" | "blob" | "arrayBuffer" | "formData";
const BODY_METHODS: ReadonlySet<BodyMethod> = new Set([
  "json",
  "text",
  "blob",
  "arrayBuffer",
  "formData",
]);

/// Wrap a Response so the first body-consumer method call fires `close`
/// after the body's promise settles (resolve OR reject). Property reads
/// other than the wrapped methods pass through to the underlying Response.
function instrumentResponseBody(response: Response, close: () => void): Response {
  return new Proxy(response, {
    get(target, prop) {
      if (typeof prop === "string" && BODY_METHODS.has(prop as BodyMethod)) {
        const fn = Reflect.get(target, prop, target) as (...args: unknown[]) => Promise<unknown>;
        return (...args: unknown[]) => fn.apply(target, args).finally(close);
      }
      const value = Reflect.get(target, prop, target);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

export async function fetchApi(
  url: string,
  options: RiposteRequestInit = {},
): Promise<Response> {
  const { hasProgress, signal, ...nativeOptions } = options;
  const loadOpts: LoadOptions = { hasProgress };
  startLoad(loadOpts);

  // Idempotent close so multiple body-method calls (or a body call after
  // an early `response.ok === false` close) don't double-decrement.
  let didClose = false;
  const close = (): void => {
    if (didClose) return;
    didClose = true;
    const aborted = signal?.aborted === true;
    endLoad(aborted ? undefined : loadOpts);
  };

  let response: Response;
  try {
    const headers: Record<string, string> = {
      ...(nativeOptions.headers as Record<string, string> | undefined),
    };

    if (["POST", "PUT", "DELETE", "PATCH"].includes((nativeOptions.method ?? "").toUpperCase())) {
      const token = await ensureCsrfToken();
      if (token) {
        headers["x-csrf-token"] = token;
      }
    }

    response = await fetch(url, {
      ...nativeOptions,
      signal,
      headers,
      credentials: "include",
    });
  } catch (err) {
    close();
    throw err;
  }

  // Non-OK responses often get checked via `response.ok` and discarded
  // without a body read. Close at headers to avoid leaking the unit.
  if (!response.ok) {
    close();
    return response;
  }

  return instrumentResponseBody(response, close);
}

import { useEffect, useRef } from "react";

/// Cloudflare Turnstile widget, rendered explicitly without a third-party React
/// wrapper. Unlike the configurator's build-time key, the social SPA reads the
/// public site key at runtime from `/api/site/config` (`turnstile_site_key`),
/// so callers pass it in. With no key the component renders nothing and the
/// form submits without a token; the server still enforces verification when
/// its secret is configured. Cloudflare's always-passing test key for local
/// work is `1x00000000000000000000AA`.
const SCRIPT_SRC = "https://challenges.cloudflare.com/turnstile/v0/api.js";
// Give up waiting for the script after this long (CSP, ad blockers, or an
// offline network can keep it from ever arriving).
const SCRIPT_TIMEOUT_MS = 10000;

declare global {
  interface Window {
    turnstile?: {
      render: (el: HTMLElement, opts: Record<string, unknown>) => string;
      remove: (id: string) => void;
    };
  }
}

interface TurnstileProps {
  /** Public site key from site config; widget renders only when present. */
  siteKey?: string;
  /** Receives the token on success, or null on error/expiry/timeout. */
  onToken: (token: string | null) => void;
  className?: string;
}

export default function Turnstile({ siteKey, onToken, className }: TurnstileProps) {
  const ref = useRef<HTMLDivElement>(null);
  const widgetId = useRef<string | null>(null);

  useEffect(() => {
    if (!siteKey) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    // Resolves true once the Turnstile API is usable, false if the script
    // fails to load or does not arrive within the timeout. Bounded by the
    // script's load/error events plus a timeout rather than an interval that
    // would poll for window.turnstile for the lifetime of the page.
    const ensureScript = () =>
      new Promise<boolean>((resolve) => {
        if (window.turnstile) return resolve(true);
        let settled = false;
        const settle = () => {
          if (settled) return;
          settled = true;
          if (timer) clearTimeout(timer);
          resolve(!!window.turnstile);
        };
        const existing = document.querySelector<HTMLScriptElement>(
          `script[src^="${SCRIPT_SRC}"]`
        );
        const script = existing ?? document.createElement("script");
        script.addEventListener("load", settle, { once: true });
        script.addEventListener("error", settle, { once: true });
        timer = setTimeout(settle, SCRIPT_TIMEOUT_MS);
        if (!existing) {
          script.src = SCRIPT_SRC;
          script.async = true;
          script.defer = true;
          document.head.appendChild(script);
        }
      });

    ensureScript().then((ready) => {
      if (cancelled || !ready || !ref.current || !window.turnstile) return;
      widgetId.current = window.turnstile.render(ref.current, {
        sitekey: siteKey,
        callback: (token: string) => onToken(token),
        "error-callback": () => onToken(null),
        "expired-callback": () => onToken(null),
        "timeout-callback": () => onToken(null),
      });
    });

    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      if (widgetId.current && window.turnstile) {
        try {
          window.turnstile.remove(widgetId.current);
        } catch {
          /* widget already gone */
        }
      }
      widgetId.current = null;
    };
  }, [siteKey, onToken]);

  if (!siteKey) return null;
  return <div ref={ref} className={className} />;
}

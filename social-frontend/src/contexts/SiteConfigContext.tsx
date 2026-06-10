import { createContext, useContext, useEffect, useState } from "react";
import { fetchApi } from "../utils/api";
import { useAuth } from "./AuthContext";

const SiteConfigContext = createContext(null);

/// Hook returning the site's runtime configuration (feature gates +
/// site_name). The shape varies by the caller's tier  anonymous and
/// commenter callers see only `site_name` + `public_feed_enabled`; posters
/// also see `poster_posting_enabled`; admins see everything.
///
/// `config` is `null` until the fetch returns successfully. While null,
/// callers must treat every gated feature as **disabled** (fail closed):
/// don't render Compose buttons, don't render the Sign-in-to-accept
/// affordance, don't render moderation links. Once `config` is non-null,
/// missing keys mean "the caller's tier doesn't get this gate" (e.g. an
/// anonymous visitor doesn't see `poster_posting_enabled`); explicit
/// `false` means the operator turned the feature off.
///
/// The context refetches whenever `useAuth().user` changes so a freshly-
/// logged-in poster gets poster-specific gates instead of the anonymous-
/// safe payload they had a moment ago.
export function useSiteConfig() {
  const ctx = useContext(SiteConfigContext);
  if (!ctx) {
    throw new Error("useSiteConfig must be used within a SiteConfigProvider");
  }
  return ctx;
}

export function SiteConfigProvider({ children }) {
  const { user } = useAuth();
  const [config, setConfig] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      // Reset state on every (re)fetch so a previous-tier payload never
      // leaks into the new one mid-transition.
      setLoading(true);
      setError(null);
      try {
        const response = await fetchApi("/api/site/config");
        if (!response.ok) {
          throw new Error(`HTTP ${response.status}`);
        }
        const data = await response.json();
        if (!cancelled) {
          setConfig(data);
        }
      } catch (err) {
        // Fail closed: leave config as-was (likely null). Consumers
        // treating null/missing keys as disabled means the UI hides
        // gated affordances until the fetch succeeds.
        if (!cancelled) {
          setError(err.message || String(err));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [user?.id, user?.role]);

  // Reflect the deployment's brand name in the browser tab. The SPA's
  // static index.html ships the platform's neutral default <title>, but
  // config is the source of truth once it loads (operator sets `site_name`
  // without a rebuild).
  useEffect(() => {
    if (config?.site_name) {
      document.title = config.site_name;
    }
  }, [config?.site_name]);

  // Source brand favicons from the configured storefront. The platform ships
  // no operator branding of its own beyond a neutral default favicon; when a
  // store is configured, point the icons at the store's root so the social app
  // matches the operator's brand. The store may be a different origin (e.g.
  // shop.* vs www.*); cross-origin icon links load fine. The expected store
  // asset paths are the contract in docs/storefront-branding.md.
  useEffect(() => {
    const shopUrl = config?.shop_url;
    if (!shopUrl) return;
    const base = shopUrl.replace(/\/+$/, "");
    const setIcon = (rel: string, type: string | null, href: string) => {
      let el = document.head.querySelector<HTMLLinkElement>(`link[rel="${rel}"]`);
      if (!el) {
        el = document.createElement("link");
        el.rel = rel;
        document.head.appendChild(el);
      }
      if (type) el.type = type;
      el.href = href;
    };
    setIcon("icon", "image/svg+xml", `${base}/favicon.svg`);
    setIcon("apple-touch-icon", null, `${base}/apple-touch-icon.png`);
  }, [config?.shop_url]);

  return (
    <SiteConfigContext.Provider value={{ config, loading, error }}>
      {children}
    </SiteConfigContext.Provider>
  );
}

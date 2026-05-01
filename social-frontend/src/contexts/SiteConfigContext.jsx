import { createContext, useContext, useEffect, useState } from "react";
import { fetchApi } from "../utils/api";
import { useAuth } from "./AuthContext";

const SiteConfigContext = createContext(null);

/// Hook returning the site's runtime configuration (feature gates +
/// site_name). The shape varies by the caller's tier — anonymous and
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

  return (
    <SiteConfigContext.Provider value={{ config, loading, error }}>
      {children}
    </SiteConfigContext.Provider>
  );
}

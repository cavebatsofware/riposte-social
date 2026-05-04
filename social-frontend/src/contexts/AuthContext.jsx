import { createContext, useContext, useEffect, useRef, useState } from "react";
import i18n from "i18next";
import { fetchApi, clearCsrfToken } from "../utils/api";
import { SUPPORTED_LOCALES } from "../i18n";

const AuthContext = createContext(null);

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return context;
}

export function AuthProvider({ children }) {
  const [user, setUser] = useState(null);
  const [loading, setLoading] = useState(true);
  const [authConfig, setAuthConfig] = useState({
    oidcEnabled: false,
    loginUrl: null,
    accountUrl: null,
  });
  // Server-driven locale sync runs at most once per page-load. After the
  // first user resolution flips i18n.changeLanguage to the saved value,
  // any subsequent re-fetch (e.g. after a profile save) shouldn't override
  // a language the user picked manually since via the LanguagePicker.
  const localeSyncedRef = useRef(false);

  useEffect(() => {
    checkAuth();
    fetchAuthConfig();
  }, []);

  // When a logged-in user has a saved server-side locale that differs from
  // the active one, switch once and write the value to localStorage so the
  // detector picks it up on subsequent visits. Anonymous viewers hit the
  // early-return; the language detector handles them.
  useEffect(() => {
    if (localeSyncedRef.current) return;
    if (!user || !user.locale) return;
    if (!SUPPORTED_LOCALES.includes(user.locale)) return;
    const active = (i18n.resolvedLanguage || i18n.language || "en").split(
      "-",
    )[0];
    if (user.locale !== active) {
      i18n.changeLanguage(user.locale);
    }
    localeSyncedRef.current = true;
  }, [user]);

  async function fetchAuthConfig() {
    try {
      const response = await fetchApi("/api/auth/config");
      if (response.ok) {
        const data = await response.json();
        setAuthConfig({
          oidcEnabled: data.oidc_enabled,
          loginUrl: data.login_url,
          accountUrl: data.account_url,
        });
      }
    } catch (error) {
      console.error("Failed to fetch auth config:", error);
    }
  }

  async function checkAuth() {
    try {
      const response = await fetchApi("/api/me");
      if (response.ok) {
        const data = await response.json();
        setUser(data);
      } else {
        setUser(null);
      }
    } catch (error) {
      console.error("Auth check failed:", error);
      setUser(null);
    } finally {
      setLoading(false);
    }
  }

  /// Password-mode login for already-activated users. OIDC mode visitors hit
  /// /api/auth/oidc/login directly via a redirect link rather than this fn.
  async function login(email, password) {
    const response = await fetchApi("/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email, password }),
    });
    if (!response.ok) {
      const data = await response.json();
      throw new Error(data.error || "Invalid email or password");
    }
    await checkAuth();
  }

  async function logout() {
    await fetchApi("/api/auth/logout", { method: "POST" });
    clearCsrfToken();
    setUser(null);
  }

  const value = {
    user,
    loading,
    authConfig,
    login,
    logout,
    refreshUser: checkAuth,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

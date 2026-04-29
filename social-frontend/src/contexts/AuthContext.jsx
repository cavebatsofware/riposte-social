import { createContext, useContext, useState, useEffect } from "react";
import { fetchApi, clearCsrfToken } from "../utils/api";

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

  useEffect(() => {
    checkAuth();
    fetchAuthConfig();
  }, []);

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

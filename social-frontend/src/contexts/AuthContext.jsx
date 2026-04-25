import React, { createContext, useContext, useState, useEffect } from "react";
import { fetchApi, clearCsrfToken } from "../utils/api";

const AuthContext = createContext(null);

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return context;
}

// TODO(Phase 1): switch endpoints from /api/admin/* to /api/auth/* + /api/me
// once the unified UserAuthBackend lands.
export function AuthProvider({ children }) {
  const [user, setUser] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    checkAuth();
  }, []);

  async function checkAuth() {
    try {
      const response = await fetchApi("/api/admin/me");
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

  async function logout() {
    await fetchApi("/api/admin/logout", { method: "POST" });
    clearCsrfToken();
    setUser(null);
  }

  const value = {
    user,
    loading,
    logout,
    refreshUser: checkAuth,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

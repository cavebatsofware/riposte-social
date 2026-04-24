import React, { createContext, useContext, useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { fetchApi, clearCsrfToken } from '../utils/api';

const AuthContext = createContext(null);

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}

export function AuthProvider({ children }) {
  const [user, setUser] = useState(null);
  const [loading, setLoading] = useState(true);
  const [authConfig, setAuthConfig] = useState({ oidcEnabled: false, loginUrl: null, accountUrl: null });
  const navigate = useNavigate();

  useEffect(() => {
    checkAuth();
    fetchAuthConfig();
  }, []);

  async function fetchAuthConfig() {
    try {
      const response = await fetchApi('/api/admin/auth-config');
      if (response.ok) {
        const data = await response.json();
        setAuthConfig({
          oidcEnabled: data.oidc_enabled,
          loginUrl: data.login_url,
          accountUrl: data.account_url,
        });
      }
    } catch (error) {
      console.error('Failed to fetch auth config:', error);
    }
  }

  async function checkAuth() {
    try {
      const response = await fetchApi('/api/admin/me');

      if (response.ok) {
        const data = await response.json();
        setUser(data);
      } else {
        setUser(null);
      }
    } catch (error) {
      console.error('Auth check failed:', error);
      setUser(null);
    } finally {
      setLoading(false);
    }
  }

  async function login(email, password) {
    const response = await fetchApi('/api/admin/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, password }),
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.error || 'Login failed');
    }

    const data = await response.json();
    setUser(data);

    // If MFA is required, redirect to MFA verification
    if (data.mfa_required) {
      navigate('/mfa-verify');
    } else if (data.force_password_change) {
      navigate('/force-password-change');
    } else {
      navigate('/dashboard');
    }
  }

  async function verifyMFA(code) {
    const response = await fetchApi('/api/admin/mfa/verify', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code }),
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.error || 'MFA verification failed');
    }

    // Refresh user data after MFA verification
    const meResponse = await fetchApi('/api/admin/me');
    if (meResponse.ok) {
      const userData = await meResponse.json();
      setUser(userData);

      // Check if password change is required
      if (userData.force_password_change) {
        navigate('/force-password-change');
      } else {
        navigate('/dashboard');
      }
    } else {
      navigate('/dashboard');
    }
  }

  async function register(email, password) {
    const response = await fetchApi('/api/admin/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, password }),
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.error || 'Registration failed');
    }

    return await response.json();
  }

  async function logout() {
    await fetchApi('/api/admin/logout', {
      method: 'POST',
    });
    clearCsrfToken();
    setUser(null);
    navigate('/login');
  }

  const value = {
    user,
    loading,
    authConfig,
    login,
    register,
    logout,
    checkAuth,
    verifyMFA,
    refreshUser: checkAuth,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

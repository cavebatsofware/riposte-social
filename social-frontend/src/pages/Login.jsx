import { useState } from "react";
import { useNavigate, Navigate } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import Layout from "../components/Layout";

/// Sign-in page for return visitors who don't have a pending invite cookie.
/// Mode-aware: in OIDC deployments it's just a button to the SSO endpoint;
/// in password deployments it's the standard email + password form.
///
/// Visitors with no account at all see the invite-only notice. There's no
/// public sign-up path; activation is gated on an admin-issued invite.
export default function Login() {
  const { user, authConfig, login, loading: authLoading } = useAuth();
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  if (user) {
    return <Navigate to="/" replace />;
  }

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");
    setSubmitting(true);
    try {
      await login(email, password);
      navigate("/");
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Layout>
      <div className="auth-card">
        <h1>Sign in</h1>

        {authLoading ? (
          <p>Loading...</p>
        ) : authConfig.oidcEnabled ? (
          <>
            <p>Sign in with your Single Sign-On account to continue.</p>
            <a className="btn-primary auth-cta" href="/api/auth/oidc/login">
              Continue to SSO
            </a>
          </>
        ) : (
          <form onSubmit={handleSubmit} className="auth-form">
            {error && <div className="alert alert-error">{error}</div>}
            <label htmlFor="login-email">Email</label>
            <input
              id="login-email"
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              autoFocus
            />
            <label htmlFor="login-password">Password</label>
            <input
              id="login-password"
              type="password"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button
              type="submit"
              className="btn-primary"
              disabled={submitting}
            >
              {submitting ? "Signing in..." : "Sign in"}
            </button>
          </form>
        )}

        <div className="auth-invite-notice">
          <p>
            <strong>Riposte Social is invite-only.</strong> If you don't have
            an account yet, ask the administrator for an invite link.
          </p>
        </div>
      </div>
    </Layout>
  );
}

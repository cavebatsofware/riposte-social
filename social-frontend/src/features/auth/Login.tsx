import { useEffect, useRef, useState } from "react";
import { useNavigate, Navigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../../contexts/AuthContext";
import Layout from "../../components/Layout";

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
  const emailRef = useRef(null);
  const { t } = useTranslation("auth");
  const { t: tCommon } = useTranslation("common");

  // Move focus into the email field once the password form mounts. Done
  // imperatively so the JSX does not need the `autoFocus` prop. Layout
  // renders focusable controls (skip link, nav, theme + language pickers,
  // hamburger) during the auth-loading window, so the focus call is gated
  // to the body element: a keyboard user who has already tabbed into the
  // header during the load doesn't get yanked into the form.
  useEffect(() => {
    if (
      !authLoading &&
      !authConfig.oidcEnabled &&
      emailRef.current &&
      document.activeElement === document.body
    ) {
      emailRef.current.focus();
    }
  }, [authLoading, authConfig.oidcEnabled]);

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
        <h1 id="login-title">{t("login.title")}</h1>

        {authLoading ? (
          <p>{tCommon("loading")}</p>
        ) : authConfig.oidcEnabled ? (
          <>
            <p>{t("login.ssoDesc")}</p>
            <a className="btn-primary auth-cta" href="/api/auth/oidc/login">
              {t("login.ssoCta")}
            </a>
          </>
        ) : (
          <form
            onSubmit={handleSubmit}
            className="auth-form"
            aria-busy={submitting}
            aria-labelledby="login-title"
          >
            {error && (
              <div className="alert alert-error" role="alert">
                {error}
              </div>
            )}
            <label htmlFor="login-email">{t("login.emailLabel")}</label>
            <input
              ref={emailRef}
              id="login-email"
              name="email"
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              autoComplete="email"
            />
            <label htmlFor="login-password">{t("login.passwordLabel")}</label>
            <input
              id="login-password"
              name="password"
              type="password"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="current-password"
            />
            <button
              type="submit"
              className="btn-primary"
              disabled={submitting}
            >
              {submitting ? t("login.submitting") : t("login.submit")}
            </button>
          </form>
        )}

        <div className="auth-invite-notice">
          <p>
            <strong>{t("login.inviteOnlyLead")}</strong>{" "}
            {t("login.inviteOnlyDetail")}
          </p>
        </div>
      </div>
    </Layout>
  );
}

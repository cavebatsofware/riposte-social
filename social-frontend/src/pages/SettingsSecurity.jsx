import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import Layout from "../components/Layout";
import "./Settings.css";

/// `/settings/security` — self-service password change + MFA management
/// for non-admin users. Mirrors the admin Profile page's password and TOTP
/// flows but lives on the social-frontend so commenters and posters don't
/// have to touch the admin UI.
///
/// OIDC-linked users see a deep-link to their identity provider's account
/// page (Keycloak's `/account`) rather than these forms — password and MFA
/// are owned by the IdP in that case.
export default function SettingsSecurity() {
  const { user, authConfig, loading: authLoading, refreshUser } = useAuth();
  const navigate = useNavigate();

  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [pwLoading, setPwLoading] = useState(false);

  const [mfaSetupData, setMfaSetupData] = useState(null);
  const [verificationCode, setVerificationCode] = useState("");
  const [disablePassword, setDisablePassword] = useState("");
  const [showDisableConfirm, setShowDisableConfirm] = useState(false);
  const [mfaLoading, setMfaLoading] = useState(false);

  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");

  useEffect(() => {
    if (!authLoading && !user) {
      navigate("/login", { replace: true });
    }
  }, [authLoading, user, navigate]);

  async function handlePasswordChange(e) {
    e.preventDefault();
    setError("");
    setSuccess("");
    if (newPassword !== confirmPassword) {
      setError("New password and confirmation don't match.");
      return;
    }
    setPwLoading(true);
    try {
      const response = await fetchApi("/api/me/password", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          current_password: currentPassword,
          new_password: newPassword,
        }),
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to change password");
      }
      setSuccess("Password changed.");
      setCurrentPassword("");
      setNewPassword("");
      setConfirmPassword("");
    } catch (err) {
      setError(err.message);
    } finally {
      setPwLoading(false);
    }
  }

  async function startMfaSetup() {
    setError("");
    setSuccess("");
    setMfaLoading(true);
    try {
      const response = await fetchApi("/api/me/mfa/setup", { method: "POST" });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to start MFA setup");
      }
      const data = await response.json();
      setMfaSetupData(data);
    } catch (err) {
      setError(err.message);
    } finally {
      setMfaLoading(false);
    }
  }

  async function confirmMfaSetup(e) {
    e.preventDefault();
    setError("");
    setMfaLoading(true);
    try {
      const response = await fetchApi("/api/me/mfa/confirm-setup", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          secret: mfaSetupData.secret,
          code: verificationCode,
        }),
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to verify code");
      }
      setSuccess("MFA enabled.");
      setMfaSetupData(null);
      setVerificationCode("");
      if (refreshUser) await refreshUser();
    } catch (err) {
      setError(err.message);
    } finally {
      setMfaLoading(false);
    }
  }

  async function disableMfa(e) {
    e.preventDefault();
    setError("");
    setMfaLoading(true);
    try {
      const response = await fetchApi("/api/me/mfa/disable", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ password: disablePassword }),
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to disable MFA");
      }
      setSuccess("MFA disabled.");
      setShowDisableConfirm(false);
      setDisablePassword("");
      if (refreshUser) await refreshUser();
    } catch (err) {
      setError(err.message);
    } finally {
      setMfaLoading(false);
    }
  }

  if (authLoading || !user) {
    return (
      <Layout>
        <p className="muted">Loading…</p>
      </Layout>
    );
  }

  return (
    <Layout>
      <header className="settings-header">
        <h1>Security settings</h1>
        <nav className="settings-tabs" aria-label="Settings sections">
          <Link to="/settings/profile" className="settings-tab">
            Profile
          </Link>
          <Link to="/settings/security" className="settings-tab active">
            Security
          </Link>
        </nav>
      </header>

      {error && <div className="alert alert-error">{error}</div>}
      {success && <div className="alert alert-success">{success}</div>}

      {authConfig.oidcEnabled ? (
        <section className="settings-section">
          <h2>Account management</h2>
          <p>
            Your password and two-factor authentication are managed through
            Single Sign-On.
          </p>
          {authConfig.accountUrl && (
            <a
              href={authConfig.accountUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="btn-primary"
            >
              Manage account in identity provider
            </a>
          )}
        </section>
      ) : (
        <>
          <section className="settings-section">
            <h2>Change password</h2>
            <form className="settings-form" onSubmit={handlePasswordChange}>
              <label htmlFor="security-current-password">
                Current password
              </label>
              <input
                id="security-current-password"
                type="password"
                value={currentPassword}
                onChange={(e) => setCurrentPassword(e.target.value)}
                autoComplete="current-password"
                required
              />
              <label htmlFor="security-new-password">New password</label>
              <input
                id="security-new-password"
                type="password"
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                autoComplete="new-password"
                required
              />
              <label htmlFor="security-confirm-password">
                Confirm new password
              </label>
              <input
                id="security-confirm-password"
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                autoComplete="new-password"
                required
              />
              <div className="settings-form-actions">
                <button
                  type="submit"
                  className="btn-primary"
                  disabled={
                    pwLoading ||
                    !currentPassword ||
                    !newPassword ||
                    !confirmPassword
                  }
                >
                  {pwLoading ? "Saving…" : "Change password"}
                </button>
              </div>
            </form>
          </section>

          <section className="settings-section">
            <h2>Two-factor authentication</h2>
            <p>
              Status:{" "}
              {user.totp_enabled ? (
                <strong>Enabled</strong>
              ) : (
                <strong>Disabled</strong>
              )}
            </p>

            {!user.totp_enabled && !mfaSetupData && (
              <button
                type="button"
                className="btn-primary"
                onClick={startMfaSetup}
                disabled={mfaLoading}
              >
                {mfaLoading ? "Setting up…" : "Enable MFA"}
              </button>
            )}

            {user.totp_enabled && !showDisableConfirm && (
              <button
                type="button"
                className="btn-secondary"
                onClick={() => setShowDisableConfirm(true)}
                disabled={mfaLoading}
              >
                Disable MFA
              </button>
            )}

            {mfaSetupData && (
              <div className="mfa-setup">
                <p>
                  Scan this QR code in your authenticator app, then enter the
                  6-digit code below.
                </p>
                <img
                  src={`data:image/png;base64,${mfaSetupData.qr_code}`}
                  alt="MFA QR Code"
                />
                <p>
                  Or enter manually: <code>{mfaSetupData.secret}</code>
                </p>
                <form onSubmit={confirmMfaSetup} className="settings-form">
                  <label htmlFor="security-totp-code">6-digit code</label>
                  <input
                    id="security-totp-code"
                    type="text"
                    value={verificationCode}
                    onChange={(e) =>
                      setVerificationCode(e.target.value.replace(/\D/g, ""))
                    }
                    maxLength={6}
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    placeholder="000000"
                  />
                  <div className="settings-form-actions">
                    <button
                      type="submit"
                      className="btn-primary"
                      disabled={mfaLoading || verificationCode.length !== 6}
                    >
                      {mfaLoading ? "Verifying…" : "Verify and enable"}
                    </button>
                    <button
                      type="button"
                      className="btn-secondary"
                      onClick={() => {
                        setMfaSetupData(null);
                        setVerificationCode("");
                      }}
                      disabled={mfaLoading}
                    >
                      Cancel
                    </button>
                  </div>
                </form>
              </div>
            )}

            {showDisableConfirm && (
              <form onSubmit={disableMfa} className="settings-form">
                <p>Confirm your password to disable MFA.</p>
                <label htmlFor="security-disable-password">Password</label>
                <input
                  id="security-disable-password"
                  type="password"
                  value={disablePassword}
                  onChange={(e) => setDisablePassword(e.target.value)}
                  autoComplete="current-password"
                  required
                />
                <div className="settings-form-actions">
                  <button
                    type="submit"
                    className="btn-secondary"
                    disabled={mfaLoading || !disablePassword}
                  >
                    {mfaLoading ? "Disabling…" : "Disable MFA"}
                  </button>
                  <button
                    type="button"
                    className="btn-secondary"
                    onClick={() => {
                      setShowDisableConfirm(false);
                      setDisablePassword("");
                    }}
                    disabled={mfaLoading}
                  >
                    Cancel
                  </button>
                </div>
              </form>
            )}
          </section>
        </>
      )}
    </Layout>
  );
}

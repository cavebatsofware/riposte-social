import React, { useState } from "react";
import Layout from "../components/Layout";
import PasswordChangeForm from "../components/PasswordChangeForm";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import "./Profile.css";

function Profile() {
  const { user, refreshUser, authConfig } = useAuth();
  const [mfaSetupData, setMfaSetupData] = useState(null);
  const [verificationCode, setVerificationCode] = useState("");
  const [disablePassword, setDisablePassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [passwordLoading, setPasswordLoading] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");
  const [showDisableConfirm, setShowDisableConfirm] = useState(false);

  async function handlePasswordChange({ currentPassword, newPassword }) {
    setPasswordLoading(true);
    setError("");
    setSuccess("");

    try {
      const response = await fetchApi("/api/admin/change-password", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          current_password: currentPassword,
          new_password: newPassword,
        }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to change password");
      }

      setSuccess("Password changed successfully!");
    } catch (err) {
      setError(err.message);
    } finally {
      setPasswordLoading(false);
    }
  }

  async function startMfaSetup() {
    setLoading(true);
    setError("");
    setSuccess("");

    try {
      const response = await fetchApi("/api/admin/mfa/setup", {
        method: "POST",
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to start MFA setup");
      }

      const data = await response.json();
      setMfaSetupData(data);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function confirmMfaSetup(e) {
    e.preventDefault();
    setLoading(true);
    setError("");

    try {
      const response = await fetchApi("/api/admin/mfa/confirm-setup", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ secret: mfaSetupData.secret, code: verificationCode }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to verify code");
      }

      setSuccess("MFA has been enabled successfully!");
      setMfaSetupData(null);
      setVerificationCode("");
      if (refreshUser) {
        await refreshUser();
      }
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function disableMfa(e) {
    e.preventDefault();
    setLoading(true);
    setError("");

    try {
      const response = await fetchApi("/api/admin/mfa/disable", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ password: disablePassword }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to disable MFA");
      }

      setSuccess("MFA has been disabled.");
      setShowDisableConfirm(false);
      setDisablePassword("");
      if (refreshUser) {
        await refreshUser();
      }
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  function cancelSetup() {
    setMfaSetupData(null);
    setVerificationCode("");
    setError("");
  }

  return (
    <Layout>
      <div className="profile-page">
        <header className="page-header">
          <h1>Profile</h1>
        </header>

        {error && <div className="alert alert-error">{error}</div>}
        {success && <div className="alert alert-success">{success}</div>}

        <section className="profile-section">
          <h2>Account Information</h2>
          <div className="profile-card">
            <div className="profile-info-row">
              <span className="profile-label">Email</span>
              <span className="profile-value">{user?.email}</span>
            </div>
            <div className="profile-info-row">
              <span className="profile-label">Email Verified</span>
              <span className="profile-value">
                {user?.email_verified ? (
                  <span className="badge badge-success">Verified</span>
                ) : (
                  <span className="badge badge-warning">Not Verified</span>
                )}
              </span>
            </div>
          </div>
        </section>

        {authConfig.oidcEnabled ? (
          <section className="profile-section">
            <h2>Account Management</h2>
            <div className="profile-card">
              <p>
                Your password and security settings are managed through Single Sign-On (SSO).
              </p>
              {authConfig.accountUrl && (
                <a
                  href={authConfig.accountUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="btn-primary"
                >
                  Manage Account in Keycloak
                </a>
              )}
            </div>
          </section>
        ) : (
          <>
            <section className="profile-section">
              <h2>Change Password</h2>
              <div className="profile-card">
                <PasswordChangeForm
                  requireCurrentPassword={true}
                  onSubmit={handlePasswordChange}
                  loading={passwordLoading}
                  email={user?.email}
                />
              </div>
            </section>

            <section className="profile-section">
              <h2>Two-Factor Authentication</h2>
              <div className="profile-card">
                <div className="mfa-status">
                  <div className="mfa-status-info">
                    <div className="mfa-status-label">Status</div>
                    <div className="mfa-status-value">
                      {user?.totp_enabled ? (
                        <span className="badge badge-success">Enabled</span>
                      ) : (
                        <span className="badge badge-gray">Disabled</span>
                      )}
                    </div>
                    <p className="mfa-description">
                      Two-factor authentication adds an extra layer of security to
                      your account by requiring a code from your authenticator app
                      when signing in.
                    </p>
                  </div>

                  {!user?.totp_enabled && !mfaSetupData && (
                    <button
                      className="btn-primary"
                      onClick={startMfaSetup}
                      disabled={loading}
                    >
                      {loading ? "Setting up..." : "Enable MFA"}
                    </button>
                  )}

                  {user?.totp_enabled && !showDisableConfirm && (
                    <button
                      className="btn-danger"
                      onClick={() => setShowDisableConfirm(true)}
                      disabled={loading}
                    >
                      Disable MFA
                    </button>
                  )}
                </div>

                {mfaSetupData && (
                  <div className="mfa-setup">
                    <h3>Set Up Authenticator App</h3>
                    <p>
                      Scan the QR code below with your authenticator app (such as
                      Google Authenticator, Authy, or 1Password).
                    </p>

                    <div className="qr-code-container">
                      <img
                        src={`data:image/png;base64,${mfaSetupData.qr_code}`}
                        alt="MFA QR Code"
                        className="qr-code"
                      />
                    </div>

                    <div className="manual-entry">
                      <p>
                        Or enter this code manually in your authenticator app:
                      </p>
                      <code className="secret-code">{mfaSetupData.secret}</code>
                    </div>

                    <form onSubmit={confirmMfaSetup} className="verify-form">
                      <div className="form-group">
                        <label htmlFor="verification-code">
                          Enter the 6-digit code from your app
                        </label>
                        <input
                          id="verification-code"
                          type="text"
                          value={verificationCode}
                          onChange={(e) =>
                            setVerificationCode(e.target.value.replace(/\D/g, ""))
                          }
                          maxLength={6}
                          placeholder="000000"
                          className="verification-input"
                          autoComplete="one-time-code"
                        />
                      </div>
                      <div className="form-actions">
                        <button
                          type="submit"
                          className="btn-primary"
                          disabled={loading || verificationCode.length !== 6}
                        >
                          {loading ? "Verifying..." : "Verify & Enable"}
                        </button>
                        <button
                          type="button"
                          className="btn-secondary"
                          onClick={cancelSetup}
                          disabled={loading}
                        >
                          Cancel
                        </button>
                      </div>
                    </form>
                  </div>
                )}

                {showDisableConfirm && (
                  <div className="mfa-disable">
                    <h3>Disable Two-Factor Authentication</h3>
                    <p className="warning-text">
                      Disabling MFA will make your account less secure. You will
                      need to enter your password to confirm.
                    </p>

                    <form onSubmit={disableMfa} className="disable-form">
                      <div className="form-group">
                        <label htmlFor="disable-password">
                          Enter your password to confirm
                        </label>
                        <input
                          id="disable-password"
                          type="password"
                          value={disablePassword}
                          onChange={(e) => setDisablePassword(e.target.value)}
                          placeholder="Your password"
                          autoComplete="current-password"
                        />
                      </div>
                      <div className="form-actions">
                        <button
                          type="submit"
                          className="btn-danger"
                          disabled={loading || !disablePassword}
                        >
                          {loading ? "Disabling..." : "Disable MFA"}
                        </button>
                        <button
                          type="button"
                          className="btn-secondary"
                          onClick={() => {
                            setShowDisableConfirm(false);
                            setDisablePassword("");
                            setError("");
                          }}
                          disabled={loading}
                        >
                          Cancel
                        </button>
                      </div>
                    </form>
                  </div>
                )}
              </div>
            </section>
          </>
        )}
      </div>
    </Layout>
  );
}

export default Profile;

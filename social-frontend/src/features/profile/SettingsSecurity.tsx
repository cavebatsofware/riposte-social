import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../../contexts/AuthContext";
import {
  changePassword,
  confirmMfaSetup as apiConfirmMfaSetup,
  disableMfa as apiDisableMfa,
  startMfaSetup as apiStartMfaSetup,
} from "./api";
import Layout from "../../components/Layout";
import "./Settings.css";

/// `/settings/security`: self-service password change + MFA management
/// for non-admin users. Mirrors the admin Profile page's password and TOTP
/// flows but lives on the social-frontend so commenters and posters don't
/// have to touch the admin UI.
///
/// OIDC-linked users see a deep-link to their identity provider's account
/// page (Keycloak's `/account`) rather than these forms; password and MFA
/// are owned by the IdP in that case.
export default function SettingsSecurity() {
  const { user, authConfig, loading: authLoading, refreshUser } = useAuth();
  const navigate = useNavigate();
  const { t } = useTranslation("settings");
  const { t: tCommon } = useTranslation("common");

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
      setError(t("security.passwordMismatch"));
      return;
    }
    setPwLoading(true);
    try {
      const response = await changePassword({
        current_password: currentPassword,
        new_password: newPassword,
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("security.passwordChangeFailed"));
      }
      setSuccess(t("security.passwordChangedSuccess"));
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
      const response = await apiStartMfaSetup();
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("security.mfaSetupFailed"));
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
      const response = await apiConfirmMfaSetup({
        secret: mfaSetupData.secret,
        code: verificationCode,
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("security.mfaVerifyFailed"));
      }
      setSuccess(t("security.mfaEnabledSuccess"));
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
      const response = await apiDisableMfa({ password: disablePassword });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("security.mfaDisableFailed"));
      }
      setSuccess(t("security.mfaDisabledSuccess"));
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
        <p className="muted">{tCommon("loading")}</p>
      </Layout>
    );
  }

  return (
    <Layout>
      <header className="settings-header">
        <h1>{t("security.title")}</h1>
        <nav className="settings-tabs" aria-label={t("tabsAria")}>
          <Link to="/settings/profile" className="settings-tab">
            {t("tabProfile")}
          </Link>
          <Link
            to="/settings/security"
            className="settings-tab active"
            aria-current="page"
          >
            {t("tabSecurity")}
          </Link>
        </nav>
      </header>

      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}
      {success && (
        <div className="alert alert-success" role="status">
          {success}
        </div>
      )}

      {authConfig.oidcEnabled ? (
        <section className="settings-section">
          <h2>{t("security.ssoHeading")}</h2>
          <p>{t("security.ssoBody")}</p>
          {authConfig.accountUrl && (
            <a
              href={authConfig.accountUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="btn-primary"
            >
              {t("security.ssoCta")}
            </a>
          )}
        </section>
      ) : (
        <>
          <section className="settings-section">
            <h2 id="security-password-heading">{t("security.passwordHeading")}</h2>
            <form
              className="settings-form"
              onSubmit={handlePasswordChange}
              aria-busy={pwLoading}
              aria-labelledby="security-password-heading"
            >
              <label htmlFor="security-current-password">
                {t("security.currentPasswordLabel")}
              </label>
              <input
                id="security-current-password"
                name="current_password"
                type="password"
                value={currentPassword}
                onChange={(e) => setCurrentPassword(e.target.value)}
                autoComplete="current-password"
                required
              />
              <label htmlFor="security-new-password">
                {t("security.newPasswordLabel")}
              </label>
              <input
                id="security-new-password"
                name="new_password"
                type="password"
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                autoComplete="new-password"
                required
              />
              <label htmlFor="security-confirm-password">
                {t("security.confirmPasswordLabel")}
              </label>
              <input
                id="security-confirm-password"
                name="confirm_password"
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
                  {pwLoading
                    ? tCommon("actions.saving")
                    : t("security.changePasswordCta")}
                </button>
              </div>
            </form>
          </section>

          <section className="settings-section">
            <h2>{t("security.mfaHeading")}</h2>
            <p>
              {t("security.mfaStatusLabel")}{" "}
              {user.totp_enabled ? (
                <strong>{t("security.mfaEnabled")}</strong>
              ) : (
                <strong>{t("security.mfaDisabled")}</strong>
              )}
            </p>

            {!user.totp_enabled && !mfaSetupData && (
              <button
                type="button"
                className="btn-primary"
                onClick={startMfaSetup}
                disabled={mfaLoading}
              >
                {mfaLoading
                  ? t("security.settingUp")
                  : t("security.enableCta")}
              </button>
            )}

            {user.totp_enabled && !showDisableConfirm && (
              <button
                type="button"
                className="btn-secondary"
                onClick={() => setShowDisableConfirm(true)}
                disabled={mfaLoading}
              >
                {t("security.disableCta")}
              </button>
            )}

            {mfaSetupData && (
              <div className="mfa-setup">
                <p>{t("security.qrInstruction")}</p>
                <img
                  src={`data:image/png;base64,${mfaSetupData.qr_code}`}
                  alt={t("security.qrAlt")}
                />
                <p>
                  {t("security.manualEntryLabel")}{" "}
                  {/* The secret is rendered as the <code> element's
                      text content so screen readers read the actual
                      characters when the user navigates to it. The
                      preceding label paragraph supplies context;
                      adding aria-label here would replace the
                      announced characters with the label string. */}
                  <code>{mfaSetupData.secret}</code>
                </p>
                <form
                  onSubmit={confirmMfaSetup}
                  className="settings-form"
                  aria-busy={mfaLoading}
                  aria-label={t("security.verifyAndEnable")}
                >
                  <label htmlFor="security-totp-code">
                    {t("security.totpCodeLabel")}
                  </label>
                  <input
                    id="security-totp-code"
                    name="totp_code"
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
                      {mfaLoading
                        ? t("security.verifying")
                        : t("security.verifyAndEnable")}
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
                      {tCommon("actions.cancel")}
                    </button>
                  </div>
                </form>
              </div>
            )}

            {showDisableConfirm && (
              <form
                onSubmit={disableMfa}
                className="settings-form"
                aria-busy={mfaLoading}
                aria-label={t("security.disableCta")}
              >
                <p>{t("security.disableConfirmPrompt")}</p>
                <label htmlFor="security-disable-password">
                  {t("security.disablePasswordLabel")}
                </label>
                <input
                  id="security-disable-password"
                  name="password"
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
                    {mfaLoading
                      ? t("security.disabling")
                      : t("security.disableCta")}
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
                    {tCommon("actions.cancel")}
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

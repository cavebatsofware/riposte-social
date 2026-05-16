import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useAuth } from "../../contexts/AuthContext";
import { acceptInvitePassword } from "./api";

/// Mode-aware acceptance UI for an already-confirmed invite. Used by both
/// `InviteSplash` (return visitors with a live cookie on `/`) and the
/// `InviteAccept` page (first-contact at `/invite/:code` after the visitor
/// consents to the cookie + trusted-device gate).
///
/// In OIDC mode this renders a button that redirects to the SSO endpoint;
/// the cookie carries the plaintext code through the redirect chain. In
/// password mode it renders an inline form that POSTs to
/// `/api/auth/invite/accept-password`. `onAccepted` is called only on
/// successful password acceptance (OIDC redirects away).
export default function InviteAcceptForm({ invite, onAccepted }) {
  const { authConfig } = useAuth();
  const { t } = useTranslation("auth");

  if (authConfig.oidcEnabled) {
    return (
      <a className="btn-primary invite-splash-cta" href="/api/auth/oidc/login">
        {t("invite.form.ssoCta")}
      </a>
    );
  }
  return <PasswordAccept invite={invite} onAccepted={onAccepted} />;
}

function PasswordAccept({ invite, onAccepted }) {
  const [email, setEmail] = useState(invite.email_hint || "");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const { t } = useTranslation("auth");

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");
    setSubmitting(true);
    try {
      const response = await acceptInvitePassword({
        code: invite.code,
        email,
        new_password: password,
      });
      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || t("invite.form.submitFailed"));
      }
      if (onAccepted) {
        await onAccepted();
      }
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="invite-splash-form"
      aria-busy={submitting}
    >
      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}
      <label htmlFor="invite-accept-email">{t("invite.form.emailLabel")}</label>
      <input
        id="invite-accept-email"
        type="email"
        required
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        placeholder={t("invite.form.emailPlaceholder")}
      />
      <label htmlFor="invite-accept-password">
        {t("invite.form.passwordLabel")}
      </label>
      <input
        id="invite-accept-password"
        type="password"
        required
        minLength={12}
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        placeholder={t("invite.form.passwordPlaceholder")}
      />
      <button type="submit" className="btn-primary" disabled={submitting}>
        {submitting ? t("invite.form.submitting") : t("invite.form.submit")}
      </button>
    </form>
  );
}

import { useState } from "react";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";

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

  if (authConfig.oidcEnabled) {
    return (
      <a className="btn-primary invite-splash-cta" href="/api/auth/oidc/login">
        Sign in to accept
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

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");
    setSubmitting(true);
    try {
      const response = await fetchApi("/api/auth/invite/accept-password", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          code: invite.code,
          email,
          new_password: password,
        }),
      });
      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to accept invite");
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
    <form onSubmit={handleSubmit} className="invite-splash-form">
      {error && <div className="alert alert-error">{error}</div>}
      <label htmlFor="invite-accept-email">Email</label>
      <input
        id="invite-accept-email"
        type="email"
        required
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        placeholder="you@example.com"
      />
      <label htmlFor="invite-accept-password">Choose a password</label>
      <input
        id="invite-accept-password"
        type="password"
        required
        minLength={12}
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        placeholder="Minimum 12 characters"
      />
      <button type="submit" className="btn-primary" disabled={submitting}>
        {submitting ? "Accepting..." : "Accept invite"}
      </button>
    </form>
  );
}

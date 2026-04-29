import { useEffect, useState } from "react";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";

/// Welcome modal that surfaces when a `pending_invite` cookie is live.
/// Polls `/api/invites/current` once on mount; if the server confirms a
/// live invite, renders an acceptance UI keyed on the auth mode:
///
/// - OIDC mode: a "Sign in" button that redirects to the OIDC login. The
///   pending_invite cookie travels with the request, so the OIDC callback
///   binds the new commenter (or admin/poster) on success.
/// - Password mode: an inline email + password form that POSTs to
///   `/api/auth/invite/accept-password`. Successful submission establishes
///   the session and refreshes the auth context.
///
/// Already-authenticated users hide the splash entirely (the cookie is also
/// cleared by the OIDC callback on successful login). The "Maybe later"
/// button calls the logout/invite endpoint to clear the cookie explicitly,
/// so the splash stops surfacing on subsequent visits.
export default function InviteSplash() {
  const { user, authConfig, refreshUser } = useAuth();
  const [invite, setInvite] = useState(null);
  const [loading, setLoading] = useState(true);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    fetchInvite();
  }, []);

  async function fetchInvite() {
    try {
      const response = await fetchApi("/api/invites/current");
      if (response.ok) {
        const data = await response.json();
        setInvite(data); // null when no live invite
      }
    } catch (err) {
      console.error("Failed to fetch invite:", err);
    } finally {
      setLoading(false);
    }
  }

  async function handleDecline() {
    try {
      await fetchApi("/api/auth/logout/invite", { method: "POST" });
    } catch (err) {
      console.error("Failed to clear invite cookie:", err);
    }
    setDismissed(true);
  }

  if (loading || dismissed || !invite || user) {
    return null;
  }

  return (
    <div className="invite-splash-overlay" role="dialog" aria-modal="true">
      <div className="invite-splash-card">
        <h2>You've been invited</h2>
        {invite.email_hint ? (
          <p>
            This invite is for <strong>{invite.email_hint}</strong>. Sign in
            to accept and start participating.
          </p>
        ) : (
          <p>
            Sign in to accept your invite and join the conversation.
          </p>
        )}

        {authConfig.oidcEnabled ? (
          <OidcAccept />
        ) : (
          <PasswordAccept
            invite={invite}
            onAccepted={async () => {
              await refreshUser();
              setDismissed(true);
            }}
          />
        )}

        <button
          type="button"
          className="invite-splash-decline"
          onClick={handleDecline}
        >
          Maybe later
        </button>
      </div>
    </div>
  );
}

function OidcAccept() {
  return (
    <a className="btn-primary invite-splash-cta" href="/api/auth/oidc/login">
      Sign in to accept
    </a>
  );
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
      await onAccepted();
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form onSubmit={handleSubmit} className="invite-splash-form">
      {error && <div className="alert alert-error">{error}</div>}
      <label htmlFor="invite-splash-email">Email</label>
      <input
        id="invite-splash-email"
        type="email"
        required
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        placeholder="you@example.com"
      />
      <label htmlFor="invite-splash-password">Choose a password</label>
      <input
        id="invite-splash-password"
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

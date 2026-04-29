import { useEffect, useState } from "react";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import InviteAcceptForm from "./InviteAcceptForm";

/// Welcome modal shown on the public feed (`/`) when a `pending_invite`
/// cookie is live. Polls `/api/invites/current` once on mount and renders
/// the shared `InviteAcceptForm` if the server confirms the cookie still
/// maps to a live invite. Already-authenticated users hide the splash.
///
/// First-contact at `/invite/{code}` is handled by the `InviteAccept` page,
/// not this component. This splash is purely for return visitors who have
/// already consented to the cookie.
export default function InviteSplash() {
  const { user, refreshUser } = useAuth();
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
          <p>Sign in to accept your invite and join the conversation.</p>
        )}

        <InviteAcceptForm
          invite={invite}
          onAccepted={async () => {
            await refreshUser();
            setDismissed(true);
          }}
        />

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

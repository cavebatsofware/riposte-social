import { useEffect, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { useAuth } from "../../contexts/AuthContext";
import { clearPendingInvite, fetchCurrentInvite } from "./api";
import { useFocusTrap } from "../../utils/useFocusTrap";
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
  const { t } = useTranslation("auth");
  const open = !loading && !dismissed && invite != null && !user;
  const trapRef = useFocusTrap(open, {
    onEscape: () => handleDecline(),
    restoreFocus: false,
  });

  useEffect(() => {
    fetchInvite();
  }, []);

  async function fetchInvite() {
    try {
      const response = await fetchCurrentInvite();
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

  function handleDecline() {
    // Dismiss synchronously so Escape and the decline button always
    // close the dialog immediately. The cookie-clearing POST is
    // best-effort and runs in the background; if it hangs or fails,
    // the splash is already gone and the user is back on the feed.
    setDismissed(true);
    clearPendingInvite().catch((err) => {
      console.error("Failed to clear invite cookie:", err);
    });
  }

  if (!open) {
    return null;
  }

  return (
    <div
      className="invite-splash-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="invite-splash-title"
      ref={trapRef}
    >
      <div className="invite-splash-card">
        <h2 id="invite-splash-title">{t("invite.splash.title")}</h2>
        {invite.email_hint ? (
          <p>
            <Trans
              i18nKey="auth:invite.splash.forRecipient"
              values={{ email: invite.email_hint }}
              components={{ bold: <strong /> }}
            />
          </p>
        ) : (
          <p>{t("invite.splash.generic")}</p>
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
          {t("invite.splash.decline")}
        </button>
      </div>
    </div>
  );
}

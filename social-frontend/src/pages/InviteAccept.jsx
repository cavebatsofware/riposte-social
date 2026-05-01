import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import InviteAcceptForm from "../components/InviteAcceptForm";
import Layout from "../components/Layout";

/// First-contact landing for `/invite/:code` links shared via email or chat.
/// Renders a combined trusted-device + cookie-consent gate before storing
/// any invite state on the device. The plaintext code stays in the URL
/// only; nothing is written to the cookie jar until the visitor explicitly
/// confirms they're on a private/trusted device and accepts the necessary
/// cookies. Visitors on shared/public computers can decline and forward
/// the link to themselves on a private device.
export default function InviteAccept() {
  const { code } = useParams();
  const { user, refreshUser } = useAuth();
  const [step, setStep] = useState("gate"); // gate | accepting | declined | invalid
  const [invite, setInvite] = useState(null);
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);

  if (user) {
    return (
      <Layout>
        <div className="invite-accept-card">
          <h1>You're already signed in</h1>
          <p>
            Signed in as <strong>{user.email}</strong>. If this invite link
            was meant for someone else, sign out first and reopen it.
          </p>
          <Link to="/" className="btn-primary">
            Go to feed
          </Link>
        </div>
      </Layout>
    );
  }

  async function handleConsent() {
    setError("");
    setSubmitting(true);
    try {
      const response = await fetchApi("/api/invites/confirm", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code }),
      });
      if (!response.ok) {
        throw new Error("Could not confirm the invite. Please try again.");
      }
      const data = await response.json();
      if (data == null) {
        setStep("invalid");
        return;
      }
      setInvite(data);
      setStep("accepting");
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Layout>
      <div className="invite-accept-card">
        {step === "gate" && (
          <>
            <h1>You've been invited</h1>
            <p>
              Welcome to <strong>Riposte Social</strong>. To complete your
              invite we need to save the code on this device for the duration
              of sign-in.
            </p>

            <h2>Are you on a trusted device?</h2>
            <p className="muted">
              If this is a shared or public computer (library, kiosk, friend's
              phone), please open the invite link on your own device instead.
              The code will stay in the URL bar of this browser; once you
              navigate away or close the tab, no trace remains.
            </p>

            <h2>Cookie notice</h2>
            <p className="muted">
              Riposte Social uses cookies that are strictly necessary for
              authentication, security (CSRF protection), and for invited
              users saving your invite state across sign-in. We do not use
              tracking, analytics, or advertising cookies.
            </p>

            {error && <div className="alert alert-error">{error}</div>}

            <div className="invite-accept-actions">
              <button
                type="button"
                className="btn-primary"
                onClick={handleConsent}
                disabled={submitting}
              >
                {submitting ? "Confirming..." : "Yes, this is my device"}
              </button>
              <button
                type="button"
                className="btn-secondary"
                onClick={() => setStep("declined")}
                disabled={submitting}
              >
                No, public/shared device
              </button>
            </div>
          </>
        )}

        {step === "accepting" && invite && (
          <>
            <h1>Accept your invite</h1>
            {invite.email_hint ? (
              <p>
                This invite is for <strong>{invite.email_hint}</strong>.
              </p>
            ) : (
              <p>Sign in to join the conversation.</p>
            )}
            <InviteAcceptForm
              invite={invite}
              onAccepted={async () => {
                await refreshUser();
              }}
            />
          </>
        )}

        {step === "declined" && (
          <>
            <h1>Open this link on your own device</h1>
            <p>
              No invite state has been saved on this browser. To accept,
              forward the link from your email to a private device and open
              it there.
            </p>
            <Link to="/" className="btn-secondary">
              Continue to public feed
            </Link>
          </>
        )}

        {step === "invalid" && (
          <>
            <h1>Invite not available</h1>
            <p>
              This invite is no longer valid. It may have expired, been
              revoked, or already been used. Ask the person who invited you
              to issue a new one.
            </p>
            <Link to="/" className="btn-secondary">
              Continue to public feed
            </Link>
          </>
        )}
      </div>
    </Layout>
  );
}

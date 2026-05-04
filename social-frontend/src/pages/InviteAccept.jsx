import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import { Trans, useTranslation } from "react-i18next";
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
  const { t } = useTranslation("auth");

  if (user) {
    return (
      <Layout>
        <div className="invite-accept-card">
          <h1>{t("invite.alreadySignedInTitle")}</h1>
          <p>
            <Trans
              i18nKey="auth:invite.alreadySignedInBody"
              values={{ email: user.email }}
              components={{ bold: <strong /> }}
            />
          </p>
          <Link to="/" className="btn-primary">
            {t("invite.goToFeed")}
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
        throw new Error(t("invite.confirmFailed"));
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
            <h1>{t("invite.youHaveBeenInvitedTitle")}</h1>
            <p>
              <Trans
                i18nKey="auth:invite.welcomeBlurb"
                components={{ bold: <strong /> }}
              />
            </p>

            <h2>{t("invite.trustedDeviceTitle")}</h2>
            <p className="muted">{t("invite.trustedDeviceBody")}</p>

            <h2>{t("invite.cookieNoticeTitle")}</h2>
            <p className="muted">{t("invite.cookieNoticeBody")}</p>

            {error && <div className="alert alert-error">{error}</div>}

            <div className="invite-accept-actions">
              <button
                type="button"
                className="btn-primary"
                onClick={handleConsent}
                disabled={submitting}
              >
                {submitting
                  ? t("invite.confirming")
                  : t("invite.trustedYes")}
              </button>
              <button
                type="button"
                className="btn-secondary"
                onClick={() => setStep("declined")}
                disabled={submitting}
              >
                {t("invite.trustedNo")}
              </button>
            </div>
          </>
        )}

        {step === "accepting" && invite && (
          <>
            <h1>{t("invite.acceptTitle")}</h1>
            {invite.email_hint ? (
              <p>
                <Trans
                  i18nKey="auth:invite.acceptForRecipient"
                  values={{ email: invite.email_hint }}
                  components={{ bold: <strong /> }}
                />
              </p>
            ) : (
              <p>{t("invite.acceptGeneric")}</p>
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
            <h1>{t("invite.declinedTitle")}</h1>
            <p>{t("invite.declinedBody")}</p>
            <Link to="/" className="btn-secondary">
              {t("invite.continueToFeed")}
            </Link>
          </>
        )}

        {step === "invalid" && (
          <>
            <h1>{t("invite.invalidTitle")}</h1>
            <p>{t("invite.invalidBody")}</p>
            <Link to="/" className="btn-secondary">
              {t("invite.continueToFeed")}
            </Link>
          </>
        )}
      </div>
    </Layout>
  );
}

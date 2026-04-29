import { useEffect, useState } from "react";

const STORAGE_KEY = "rs_cookie_ack_v1";

/// Lightweight cookie disclosure banner. Riposte Social only uses cookies
/// that are strictly necessary for authentication, CSRF protection, and
/// (for invited users) carrying invite state across sign-in. There are no
/// tracking, analytics, or advertising cookies.
///
/// Acknowledgment is persisted in localStorage so the banner shows once
/// per device. The banner does not gate the site itself; the strictly-
/// necessary cookies still flow whether or not the visitor clicks OK,
/// per ePrivacy guidance for essential cookies. Tracking-grade cookies
/// (none yet) would need an opt-in here.
export default function CookieBanner() {
  const [acknowledged, setAcknowledged] = useState(true);

  useEffect(() => {
    try {
      setAcknowledged(localStorage.getItem(STORAGE_KEY) === "1");
    } catch {
      // Browsers in private mode without storage access. Don't block.
      setAcknowledged(true);
    }
  }, []);

  function handleAck() {
    try {
      localStorage.setItem(STORAGE_KEY, "1");
    } catch {
      // ignore; banner just won't persist
    }
    setAcknowledged(true);
  }

  if (acknowledged) {
    return null;
  }

  return (
    <div className="cookie-banner" role="region" aria-label="Cookie notice">
      <p>
        Riposte Social uses cookies that are strictly necessary for sign-in
        and security. We do not use tracking, analytics, or advertising
        cookies.
      </p>
      <button type="button" className="btn-primary" onClick={handleAck}>
        OK
      </button>
    </div>
  );
}

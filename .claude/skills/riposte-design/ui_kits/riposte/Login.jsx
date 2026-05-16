/* global React */
const { useState: useStateLg } = React;

/// Sign-in card. Password mode only in the demo; the real app also
/// supports an OIDC redirect path when the deployment enables it.
function Login({ onLogin, onCancel }) {
  const [email, setEmail] = useStateLg("");
  const [password, setPassword] = useStateLg("");
  const [error, setError] = useStateLg("");
  const [submitting, setSubmitting] = useStateLg(false);

  function submit(e) {
    e.preventDefault();
    if (!email || !password) {
      setError("Email and password are required.");
      return;
    }
    setSubmitting(true);
    setTimeout(() => {
      onLogin({ email });
      setSubmitting(false);
    }, 400);
  }

  return (
    <div className="auth-card">
      <h1>Sign in</h1>
      <form onSubmit={submit} className="auth-form" aria-busy={submitting}>
        {error && <div className="alert-error" role="alert">{error}</div>}
        <label htmlFor="login-email">Email</label>
        <input id="login-email" type="email" autoComplete="email" required value={email} onChange={(e) => setEmail(e.target.value)} />
        <label htmlFor="login-password">Password</label>
        <input id="login-password" type="password" autoComplete="current-password" required value={password} onChange={(e) => setPassword(e.target.value)} />
        <button type="submit" className="btn-primary" disabled={submitting}>
          {submitting ? "Signing in…" : "Sign in"}
        </button>
      </form>
      <div className="auth-invite-notice">
        <p><strong>Riposte Social is invite-only.</strong> If you don't have an account yet, ask the administrator for an invite link.</p>
      </div>
    </div>
  );
}

/// Welcome modal shown on the public feed when a pending invite cookie is live.
function InviteSplash({ inviteEmail, onAccept, onDecline }) {
  return (
    <div className="invite-splash-overlay" role="dialog" aria-modal="true" aria-labelledby="invite-splash-title">
      <div className="invite-splash-card">
        <h2 id="invite-splash-title">You've been invited</h2>
        {inviteEmail ? (
          <p>This invite is for <strong>{inviteEmail}</strong>. Sign in to accept and start participating.</p>
        ) : (
          <p>Sign in to accept your invite and join the conversation.</p>
        )}
        <div style={{ marginTop: "var(--spacing-4)" }}>
          <button className="btn-primary" style={{ width: "100%" }} onClick={onAccept}>Sign in to accept</button>
        </div>
        <button className="invite-splash-decline" onClick={onDecline}>Maybe later</button>
      </div>
    </div>
  );
}

Object.assign(window, { Login, InviteSplash });

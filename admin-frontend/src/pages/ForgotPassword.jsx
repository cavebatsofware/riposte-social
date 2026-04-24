import { useState } from "react";
import { Link } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import "./ForgotPassword.css";

function ForgotPassword() {
  const { authConfig } = useAuth();
  const [step, setStep] = useState("email"); // "email" | "mfa" | "success"
  const [email, setEmail] = useState("");
  const [mfaCode, setMfaCode] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function handleEmailSubmit(e) {
    e.preventDefault();
    setLoading(true);
    setError("");

    try {
      const response = await fetchApi("/api/admin/forgot-password", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to initiate password reset");
      }

      // Always proceed to MFA step (server returns requires_mfa: true regardless of email validity)
      setStep("mfa");
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function handleMfaSubmit(e) {
    e.preventDefault();
    setLoading(true);
    setError("");

    try {
      const response = await fetchApi("/api/admin/forgot-password/verify-mfa", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, code: mfaCode }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "MFA verification failed");
      }

      setStep("success");
    } catch (err) {
      setError(err.message);
      setMfaCode("");
    } finally {
      setLoading(false);
    }
  }

  if (authConfig.oidcEnabled) {
    return (
      <div className="forgot-password-page">
        <div className="forgot-password-container">
          <div className="forgot-password-card">
            <div className="forgot-password-header">
              <h1>Reset Password</h1>
              <p>Password management is handled through Single Sign-On (SSO).</p>
              <p>Please use your SSO provider to reset your password.</p>
            </div>
            <div className="forgot-password-footer">
              <Link to="/login">Back to Login</Link>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="forgot-password-page">
      <div className="forgot-password-container">
        <div className="forgot-password-card">
          <div className="forgot-password-header">
            <h1>Reset Password</h1>
            {step === "email" && (
              <p>Enter your email address to begin the password reset process.</p>
            )}
            {step === "mfa" && (
              <p>
                Enter the 6-digit code from your authenticator app to verify your
                identity.
              </p>
            )}
            {step === "success" && (
              <p>Check your email for password reset instructions.</p>
            )}
          </div>

          {error && <div className="alert alert-error">{error}</div>}

          {step === "email" && (
            <form onSubmit={handleEmailSubmit} className="forgot-password-form">
              <div className="form-group">
                <label htmlFor="email">Email Address</label>
                <input
                  id="email"
                  type="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="admin@example.com"
                  required
                  autoComplete="email"
                  autoFocus
                />
              </div>
              <button
                type="submit"
                className="btn-primary btn-full"
                disabled={loading || !email}
              >
                {loading ? "Sending..." : "Continue"}
              </button>
            </form>
          )}

          {step === "mfa" && (
            <form onSubmit={handleMfaSubmit} className="forgot-password-form">
              <div className="form-group">
                <label htmlFor="mfa-code">Authentication Code</label>
                <input
                  id="mfa-code"
                  type="text"
                  value={mfaCode}
                  onChange={(e) => setMfaCode(e.target.value.replace(/\D/g, ""))}
                  maxLength={6}
                  placeholder="000000"
                  className="mfa-input"
                  autoComplete="one-time-code"
                  autoFocus
                />
                <p className="form-hint">
                  Enter the code from your authenticator app (Google Authenticator,
                  Authy, etc.)
                </p>
              </div>
              <button
                type="submit"
                className="btn-primary btn-full"
                disabled={loading || mfaCode.length !== 6}
              >
                {loading ? "Verifying..." : "Verify & Send Reset Email"}
              </button>
              <button
                type="button"
                className="btn-secondary btn-full"
                onClick={() => {
                  setStep("email");
                  setMfaCode("");
                  setError("");
                }}
                disabled={loading}
              >
                Back
              </button>
            </form>
          )}

          {step === "success" && (
            <div className="success-message">
              <div className="success-icon">&#10003;</div>
              <p>
                If an account exists with that email and has MFA enabled, you will
                receive a password reset link shortly.
              </p>
              <p className="success-note">
                The link will expire in 1 hour. Check your spam folder if you don't
                see the email.
              </p>
            </div>
          )}

          <div className="forgot-password-footer">
            <Link to="/login">Back to Login</Link>
          </div>
        </div>
      </div>
    </div>
  );
}

export default ForgotPassword;

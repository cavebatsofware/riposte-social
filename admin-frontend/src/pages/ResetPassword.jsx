import { useState, useEffect } from "react";
import { Link, useSearchParams, useNavigate } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import PasswordChangeForm from "../components/PasswordChangeForm";
import "./ResetPassword.css";

function ResetPassword() {
  const { authConfig } = useAuth();
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState(false);
  const token = searchParams.get("token");

  useEffect(() => {
    if (!token) {
      setError("Invalid or missing reset token. Please request a new password reset.");
    }
  }, [token]);

  async function handleSubmit({ newPassword }) {
    setLoading(true);
    setError("");

    try {
      const response = await fetchApi("/api/admin/reset-password", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ token, new_password: newPassword }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to reset password");
      }

      setSuccess(true);
      // Redirect to login after a brief delay
      setTimeout(() => {
        navigate("/login");
      }, 3000);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  if (authConfig.oidcEnabled) {
    return (
      <div className="reset-password-page">
        <div className="reset-password-container">
          <div className="reset-password-card">
            <div className="reset-password-header">
              <h1>Set New Password</h1>
              <p>Password management is handled through Single Sign-On (SSO).</p>
            </div>
            <div className="reset-password-footer">
              <Link to="/login">Back to Login</Link>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="reset-password-page">
      <div className="reset-password-container">
        <div className="reset-password-card">
          <div className="reset-password-header">
            <h1>Set New Password</h1>
            {!success && (
              <p>Enter your new password below.</p>
            )}
          </div>

          {error && <div className="alert alert-error">{error}</div>}

          {success ? (
            <div className="success-message">
              <div className="success-icon">&#10003;</div>
              <p>Your password has been reset successfully!</p>
              <p className="success-note">
                Redirecting to login page...
              </p>
              <Link to="/login" className="btn-primary btn-full">
                Go to Login
              </Link>
            </div>
          ) : token ? (
            <PasswordChangeForm
              requireCurrentPassword={false}
              onSubmit={handleSubmit}
              loading={loading}
              email=""
            />
          ) : (
            <div className="invalid-token">
              <p>
                The reset link is invalid or has expired. Please request a new
                password reset.
              </p>
              <Link to="/forgot-password" className="btn-primary btn-full">
                Request New Reset
              </Link>
            </div>
          )}

          <div className="reset-password-footer">
            <Link to="/login">Back to Login</Link>
          </div>
        </div>
      </div>
    </div>
  );
}

export default ResetPassword;

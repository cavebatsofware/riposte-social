import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import PasswordChangeForm from "../components/PasswordChangeForm";
import "./ForcePasswordChange.css";

function ForcePasswordChange() {
  const { user, logout, refreshUser } = useAuth();
  const navigate = useNavigate();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function handleSubmit({ currentPassword, newPassword }) {
    setLoading(true);
    setError("");

    try {
      const response = await fetchApi("/api/admin/change-password", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          current_password: currentPassword,
          new_password: newPassword,
        }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to change password");
      }

      // Refresh user data to clear force_password_change flag
      if (refreshUser) {
        await refreshUser();
      }

      // Redirect to dashboard
      navigate("/dashboard");
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function handleLogout() {
    try {
      await logout();
      navigate("/login");
    } catch (err) {
      console.error("Logout failed:", err);
    }
  }

  return (
    <div className="force-password-change-page">
      <div className="force-password-change-container">
        <div className="force-password-change-card">
          <div className="force-password-change-header">
            <div className="warning-icon">!</div>
            <h1>Password Change Required</h1>
            <p>
              Your password was set by an administrator. For security reasons, you
              must choose a new password before continuing.
            </p>
          </div>

          {error && <div className="alert alert-error">{error}</div>}

          <PasswordChangeForm
            requireCurrentPassword={true}
            onSubmit={handleSubmit}
            loading={loading}
            email={user?.email}
          />

          <div className="force-password-change-footer">
            <button
              type="button"
              className="btn-link"
              onClick={handleLogout}
              disabled={loading}
            >
              Logout Instead
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export default ForcePasswordChange;

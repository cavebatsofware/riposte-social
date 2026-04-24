import React, { useState, useEffect } from "react";
import Layout from "../components/Layout";
import Table from "../components/Table";
import PasswordChangeForm from "../components/PasswordChangeForm";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import "./AdminUsers.css";

function AdminUsers() {
  const { user: currentUser } = useAuth();
  const [users, setUsers] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");
  const [currentPage, setCurrentPage] = useState(1);
  const [totalPages, setTotalPages] = useState(1);
  const [total, setTotal] = useState(0);
  const [editingUser, setEditingUser] = useState(null);
  const [showEditModal, setShowEditModal] = useState(false);

  useEffect(() => {
    fetchUsers(currentPage);
  }, [currentPage]);

  async function fetchUsers(page = 1) {
    try {
      setLoading(true);
      const response = await fetchApi(
        `/api/admin/users?page=${page}&per_page=20`
      );

      if (!response.ok) {
        throw new Error("Failed to fetch admin users");
      }

      const data = await response.json();
      setUsers(data.data);
      setTotal(data.total);
      setTotalPages(data.total_pages);
      setCurrentPage(data.page);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  function handlePageChange(newPage) {
    if (newPage >= 1 && newPage <= totalPages) {
      setCurrentPage(newPage);
    }
  }

  function formatDate(dateString) {
    if (!dateString) return "N/A";
    const date = new Date(dateString);
    return date.toLocaleDateString();
  }

  function openEditModal(user) {
    setEditingUser(user);
    setShowEditModal(true);
    setError("");
    setSuccess("");
  }

  function closeEditModal() {
    setShowEditModal(false);
    setEditingUser(null);
  }

  async function handleUpdateUser(userId, updates) {
    try {
      const response = await fetchApi(`/api/admin/users/${userId}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(updates),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to update user");
      }

      setSuccess("User updated successfully");
      closeEditModal();
      await fetchUsers(currentPage);
    } catch (err) {
      setError(err.message);
    }
  }

  const columns = [
    {
      key: "email",
      header: "Email",
      render: (value, row) => (
        <span className={row.id === currentUser?.id ? "current-user-email" : ""}>
          {value}
          {row.id === currentUser?.id && (
            <span className="badge badge-info badge-small">You</span>
          )}
        </span>
      ),
    },
    {
      key: "active",
      header: "Status",
      render: (value) => (
        <span className={`badge ${value ? "badge-success" : "badge-danger"}`}>
          {value ? "Active" : "Deactivated"}
        </span>
      ),
    },
    {
      key: "email_verified",
      header: "Email Verified",
      render: (value) => (
        <span className={`badge ${value ? "badge-success" : "badge-warning"}`}>
          {value ? "Verified" : "Pending"}
        </span>
      ),
    },
    {
      key: "totp_enabled",
      header: "MFA",
      render: (value) => (
        <span className={`badge ${value ? "badge-success" : "badge-gray"}`}>
          {value ? "Enabled" : "Disabled"}
        </span>
      ),
    },
    {
      key: "mfa_locked",
      header: "MFA Status",
      render: (value, row) =>
        value ? (
          <span className="badge badge-danger">
            Locked ({row.mfa_failed_attempts} attempts)
          </span>
        ) : (
          <span className="badge badge-success">OK</span>
        ),
    },
    {
      key: "created_at",
      header: "Created",
      render: (value) => formatDate(value),
    },
    {
      key: "actions",
      header: "Actions",
      render: (_, row) => {
        if (row.id === currentUser?.id) {
          return <span className="text-muted">Use Profile</span>;
        }
        return (
          <button
            onClick={() => openEditModal(row)}
            className="btn-secondary btn-sm"
          >
            Edit
          </button>
        );
      },
    },
  ];

  // Calculate stats
  const deactivatedCount = users.filter((u) => !u.active).length;
  const unverifiedCount = users.filter((u) => !u.email_verified).length;
  const mfaDisabledCount = users.filter((u) => !u.totp_enabled).length;
  const lockedOutCount = users.filter((u) => u.mfa_locked).length;

  return (
    <Layout>
      <div className="admin-users-page">
        <header className="page-header">
          <h1>Admin Users</h1>
        </header>

        {error && <div className="alert alert-error">{error}</div>}
        {success && <div className="alert alert-success">{success}</div>}

        <div className="users-stats">
          <div className="stat-card">
            <div className="stat-label">Total Users</div>
            <div className="stat-value">{total}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Deactivated</div>
            <div className="stat-value">{deactivatedCount}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Unverified</div>
            <div className="stat-value">{unverifiedCount}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">MFA Disabled</div>
            <div className="stat-value">{mfaDisabledCount}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Locked Out</div>
            <div className="stat-value">{lockedOutCount}</div>
          </div>
        </div>

        <Table
          columns={columns}
          data={users}
          loading={loading}
          emptyMessage="No admin users found."
          getRowClassName={(row) =>
            row.id === currentUser?.id ? "current-user-row" : ""
          }
          pagination={{
            page: currentPage,
            totalPages,
            onPageChange: handlePageChange,
          }}
        />

        {showEditModal && editingUser && (
          <EditUserModal
            user={editingUser}
            onSave={handleUpdateUser}
            onClose={closeEditModal}
          />
        )}
      </div>
    </Layout>
  );
}

function EditUserModal({ user, onSave, onClose }) {
  const [updates, setUpdates] = useState({});
  const [loading, setLoading] = useState(false);
  const [showPasswordForm, setShowPasswordForm] = useState(false);
  const [passwordLoading, setPasswordLoading] = useState(false);
  const [resendLoading, setResendLoading] = useState(false);
  const [resendSuccess, setResendSuccess] = useState(false);
  const [resendError, setResendError] = useState("");

  async function handleSubmit() {
    setLoading(true);
    await onSave(user.id, updates);
    setLoading(false);
  }

  async function handlePasswordChange({ newPassword }) {
    setPasswordLoading(true);
    await onSave(user.id, { new_password: newPassword });
    setPasswordLoading(false);
    setShowPasswordForm(false);
  }

  async function handleResendVerification() {
    setResendLoading(true);
    setResendError("");
    setResendSuccess(false);
    try {
      const response = await fetchApi(`/api/admin/users/${user.id}/resend-verification`, {
        method: "POST",
      });
      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to resend verification email");
      }
      setResendSuccess(true);
    } catch (err) {
      setResendError(err.message);
    } finally {
      setResendLoading(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content modal-large" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Edit User</h2>
          <button className="modal-close" onClick={onClose}>
            &times;
          </button>
        </div>
        <div className="modal-body">
            <div className="user-info-summary">
              <strong>Email:</strong> {user.email}
              {user.force_password_change && (
                <span className="badge badge-warning badge-small" style={{ marginLeft: '8px' }}>
                  Password Change Required
                </span>
              )}
            </div>

            <div className="setting-item">
              <div className="setting-info">
                <div className="setting-label">Account Status</div>
                <div className="setting-description">
                  {user.active
                    ? "Deactivating will prevent login and clear MFA settings"
                    : "Reactivating will require email re-verification"}
                </div>
              </div>
              <div className="setting-control">
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    checked={updates.active ?? user.active}
                    onChange={(e) =>
                      setUpdates({ ...updates, active: e.target.checked })
                    }
                  />
                  <span className="toggle-slider"></span>
                </label>
                <span className="setting-value">
                  {(updates.active ?? user.active) ? "Active" : "Deactivated"}
                </span>
              </div>
            </div>

            <div className="setting-item">
              <div className="setting-info">
                <div className="setting-label">Email Verified</div>
                <div className="setting-description">
                  Mark this user's email as verified
                </div>
              </div>
              <div className="setting-control">
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    checked={updates.email_verified ?? user.email_verified}
                    onChange={(e) =>
                      setUpdates({ ...updates, email_verified: e.target.checked })
                    }
                  />
                  <span className="toggle-slider"></span>
                </label>
                <span className="setting-value">
                  {(updates.email_verified ?? user.email_verified)
                    ? "Verified"
                    : "Pending"}
                </span>
              </div>
            </div>

            {!user.email_verified && user.active && (
              <div className="setting-item">
                <div className="setting-info">
                  <div className="setting-label">Resend Verification Email</div>
                  <div className="setting-description">
                    Send a new verification email to this user (generates a fresh 24-hour token)
                  </div>
                </div>
                <div className="setting-control">
                  {resendSuccess ? (
                    <span className="badge badge-success">Email sent!</span>
                  ) : (
                    <button
                      type="button"
                      className="btn-secondary btn-sm"
                      onClick={handleResendVerification}
                      disabled={resendLoading}
                    >
                      {resendLoading ? "Sending..." : "Resend Email"}
                    </button>
                  )}
                </div>
                {resendError && (
                  <div className="setting-error">{resendError}</div>
                )}
              </div>
            )}

            {user.mfa_locked && (
              <div className="setting-item">
                <div className="setting-info">
                  <div className="setting-label">Reset MFA Lockout</div>
                  <div className="setting-description">
                    Clears failed attempts and removes lockout (currently{" "}
                    {user.mfa_failed_attempts} failed attempts)
                  </div>
                </div>
                <div className="setting-control">
                  <label className="toggle-switch">
                    <input
                      type="checkbox"
                      checked={updates.reset_mfa_lockout || false}
                      onChange={(e) =>
                        setUpdates({
                          ...updates,
                          reset_mfa_lockout: e.target.checked,
                        })
                      }
                    />
                    <span className="toggle-slider"></span>
                  </label>
                  <span className="setting-value">
                    {updates.reset_mfa_lockout ? "Reset" : "Locked"}
                  </span>
                </div>
              </div>
            )}

            {user.totp_enabled && (
              <div className="setting-item danger-setting">
                <div className="setting-info">
                  <div className="setting-label danger-label">Disable MFA</div>
                  <div className="setting-description warning-text">
                    This will completely disable MFA for this user. They will need
                    to set it up again. Use this for account recovery only.
                  </div>
                </div>
                <div className="setting-control">
                  <label className="toggle-switch">
                    <input
                      type="checkbox"
                      checked={updates.disable_mfa || false}
                      onChange={(e) =>
                        setUpdates({ ...updates, disable_mfa: e.target.checked })
                      }
                    />
                    <span className="toggle-slider"></span>
                  </label>
                  <span className="setting-value">
                    {updates.disable_mfa ? "Disable" : "Enabled"}
                  </span>
                </div>
              </div>
            )}

            <div className="setting-section">
              <h3>Set New Password</h3>
              <p className="setting-description">
                Setting a new password will require the user to change it on next login.
              </p>
              {!showPasswordForm ? (
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={() => setShowPasswordForm(true)}
                >
                  Set Password
                </button>
              ) : (
                <div className="password-form-container">
                  <PasswordChangeForm
                    requireCurrentPassword={false}
                    onSubmit={handlePasswordChange}
                    loading={passwordLoading}
                    email={user.email}
                  />
                  <button
                    type="button"
                    className="btn-secondary btn-sm"
                    onClick={() => setShowPasswordForm(false)}
                    style={{ marginTop: '8px' }}
                  >
                    Cancel
                  </button>
                </div>
              )}
            </div>
        </div>
        <div className="modal-footer">
          <button
            type="button"
            className="btn-secondary"
            onClick={onClose}
            disabled={loading || passwordLoading}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn-primary"
            onClick={handleSubmit}
            disabled={loading || Object.keys(updates).length === 0}
          >
            {loading ? "Saving..." : "Save Changes"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default AdminUsers;

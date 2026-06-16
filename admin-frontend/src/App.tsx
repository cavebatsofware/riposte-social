import { useEffect, useState, Suspense, lazy } from "react";
import { Routes, Route, Navigate } from "react-router-dom";
import { AuthProvider, useAuth } from "./contexts/AuthContext";
import { fetchApi } from "./utils/api";
import "./App.css";

// Each route is code-split into its own chunk fetched on first visit, so the
// login/initial payload no longer carries every page (notably the dashboard's
// charting library). A Suspense boundary below covers the chunk load.
const Login = lazy(() => import("./features/auth/Login"));
const Register = lazy(() => import("./features/auth/Register"));
const VerifyEmail = lazy(() => import("./features/auth/VerifyEmail"));
const MFAVerify = lazy(() => import("./features/auth/MFAVerify"));
const ForgotPassword = lazy(() => import("./features/auth/ForgotPassword"));
const ResetPassword = lazy(() => import("./features/auth/ResetPassword"));
const ForcePasswordChange = lazy(() => import("./features/auth/ForcePasswordChange"));
const Dashboard = lazy(() => import("./features/dashboard/Dashboard"));
const AccessCodes = lazy(() => import("./features/access-codes/AccessCodes"));
const AccessLogs = lazy(() => import("./features/access-logs/AccessLogs"));
const Orders = lazy(() => import("./features/orders/Orders"));
const Users = lazy(() => import("./features/users/Users"));
const Imports = lazy(() => import("./features/imports/Imports"));
const InviteCodes = lazy(() => import("./features/invites/InviteCodes"));
const Moderation = lazy(() => import("./features/moderation/Moderation"));
const Settings = lazy(() => import("./features/settings/Settings"));
const Profile = lazy(() => import("./features/profile/Profile"));

function ProtectedRoute({ children, allowForcePasswordChange = false }) {
  const { user, loading, logout } = useAuth();

  if (loading) {
    return <div className="loading">Loading...</div>;
  }

  if (!user) {
    return <Navigate to="/login" replace />;
  }

  // If MFA is required but not verified, redirect to MFA verification
  if (user.mfa_required) {
    return <Navigate to="/mfa-verify" replace />;
  }

  // If password change is required, redirect to force password change page
  // Unless we're already on that page (allowForcePasswordChange=true)
  if (user.force_password_change && !allowForcePasswordChange) {
    return <Navigate to="/force-password-change" replace />;
  }

  // If user does not have the administrator role, show unauthorized page
  if (user.role && user.role !== "administrator") {
    return (
      <div className="container">
        <div className="card">
          <h1>Access Denied</h1>
          <p>
            You are signed in as <strong>{user.email}</strong>, but your account
            does not have the administrator role required to access this panel.
          </p>
          <p>Please contact your administrator to request access.</p>
          <button className="btn" onClick={logout}>
            Sign Out
          </button>
        </div>
      </div>
    );
  }

  return children;
}

// Gate the orders route on the backend `commerce_enabled` flag so a social-only
// build (no `business` feature, so no /api/admin/orders routes) does not surface
// a page that only errors.
function CommerceRoute({ children }) {
  const [status, setStatus] = useState("loading");

  useEffect(() => {
    let cancelled = false;
    fetchApi("/api/site/config")
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => {
        if (!cancelled) setStatus(d?.commerce_enabled ? "on" : "off");
      })
      .catch(() => {
        if (!cancelled) setStatus("off");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (status === "loading") {
    return <div className="loading">Loading...</div>;
  }
  if (status === "off") {
    return <Navigate to="/dashboard" replace />;
  }
  return children;
}

function App() {
  return (
    <AuthProvider>
      <div className="app">
        {/* Covers the active route's lazy chunk load with the same loading
            state ProtectedRoute uses, so navigation degrades gracefully. */}
        <Suspense fallback={<div className="loading">Loading...</div>}>
          <Routes>
          <Route path="/login" element={<Login />} />
          <Route path="/register" element={<Register />} />
          <Route path="/verify-email" element={<VerifyEmail />} />
          <Route path="/forgot-password" element={<ForgotPassword />} />
          <Route path="/reset-password" element={<ResetPassword />} />
          <Route
            path="/force-password-change"
            element={
              <ProtectedRoute allowForcePasswordChange={true}>
                <ForcePasswordChange />
              </ProtectedRoute>
            }
          />
          <Route
            path="/dashboard"
            element={
              <ProtectedRoute>
                <Dashboard />
              </ProtectedRoute>
            }
          />
          <Route
            path="/access-codes"
            element={
              <ProtectedRoute>
                <AccessCodes />
              </ProtectedRoute>
            }
          />
          <Route
            path="/access-logs"
            element={
              <ProtectedRoute>
                <AccessLogs />
              </ProtectedRoute>
            }
          />
          <Route
            path="/orders"
            element={
              <ProtectedRoute>
                <CommerceRoute>
                  <Orders />
                </CommerceRoute>
              </ProtectedRoute>
            }
          />
          <Route
            path="/users"
            element={
              <ProtectedRoute>
                <Users />
              </ProtectedRoute>
            }
          />
          <Route
            path="/invite-codes"
            element={
              <ProtectedRoute>
                <InviteCodes />
              </ProtectedRoute>
            }
          />
          <Route
            path="/moderation"
            element={
              <ProtectedRoute>
                <Moderation />
              </ProtectedRoute>
            }
          />
          <Route
            path="/imports"
            element={
              <ProtectedRoute>
                <Imports />
              </ProtectedRoute>
            }
          />
          <Route
            path="/settings"
            element={
              <ProtectedRoute>
                <Settings />
              </ProtectedRoute>
            }
          />
          <Route
            path="/profile"
            element={
              <ProtectedRoute>
                <Profile />
              </ProtectedRoute>
            }
          />
          <Route path="/mfa-verify" element={<MFAVerify />} />
          <Route path="/" element={<Navigate to="/dashboard" replace />} />
          </Routes>
        </Suspense>
      </div>
    </AuthProvider>
  );
}

export default App;

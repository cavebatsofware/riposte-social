import { Routes, Route, Navigate } from "react-router-dom";
import { AuthProvider, useAuth } from "./contexts/AuthContext";
import Login from "./features/auth/Login";
import Register from "./features/auth/Register";
import VerifyEmail from "./features/auth/VerifyEmail";
import MFAVerify from "./features/auth/MFAVerify";
import ForgotPassword from "./features/auth/ForgotPassword";
import ResetPassword from "./features/auth/ResetPassword";
import ForcePasswordChange from "./features/auth/ForcePasswordChange";
import Dashboard from "./features/dashboard/Dashboard";
import AccessCodes from "./features/access-codes/AccessCodes";
import AccessLogs from "./features/access-logs/AccessLogs";
import Orders from "./features/orders/Orders";
import Users from "./features/users/Users";
import Imports from "./features/imports/Imports";
import InviteCodes from "./features/invites/InviteCodes";
import Moderation from "./features/moderation/Moderation";
import Settings from "./features/settings/Settings";
import Profile from "./features/profile/Profile";
import "./App.css";

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

function App() {
  return (
    <AuthProvider>
      <div className="app">
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
                <Orders />
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
      </div>
    </AuthProvider>
  );
}

export default App;

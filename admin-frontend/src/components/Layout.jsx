import React from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import "./Layout.css";

function Layout({ children }) {
  const { user, logout } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();

  const isActive = (path) => {
    return location.pathname === path;
  };

  return (
    <div className="layout">
      <header className="layout-header">
        <div className="header-content">
          <div className="header-left">
            <h1 onClick={() => navigate("/dashboard")} className="site-title">
              Admin Dashboard
            </h1>
            <nav className="header-nav">
              <button
                className={`nav-link ${isActive("/dashboard") ? "active" : ""}`}
                onClick={() => navigate("/dashboard")}
              >
                Dashboard
              </button>
              {user?.features?.access_codes_enabled !== false && (
                <button
                  className={`nav-link ${isActive("/access-codes") ? "active" : ""}`}
                  onClick={() => navigate("/access-codes")}
                >
                  Access Codes
                </button>
              )}
              <button
                className={`nav-link ${isActive("/access-logs") ? "active" : ""}`}
                onClick={() => navigate("/access-logs")}
              >
                Access Logs
              </button>
              <button
                className={`nav-link ${isActive("/admin-users") ? "active" : ""}`}
                onClick={() => navigate("/admin-users")}
              >
                Admin Users
              </button>
              <button
                className={`nav-link ${isActive("/settings") ? "active" : ""}`}
                onClick={() => navigate("/settings")}
              >
                Settings
              </button>
              <button
                className={`nav-link ${isActive("/profile") ? "active" : ""}`}
                onClick={() => navigate("/profile")}
              >
                Profile
              </button>
            </nav>
          </div>
          <div className="header-right">
            <span className="user-email">{user?.email}</span>
            <button onClick={logout} className="btn-logout">
              Logout
            </button>
          </div>
        </div>
      </header>

      <main className="layout-main">{children}</main>
    </div>
  );
}

export default Layout;

import React, { useEffect, useRef, useState } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import "./Layout.css";

/// Admin shell. Brand + account-menu in a slim top header; primary
/// navigation lives in a left sidebar with section groupings (Content,
/// Users, Data, System) so the bar stops getting crowded as new pages
/// land. On narrow viewports the sidebar collapses behind a hamburger
/// button that opens a slide-in drawer.
function Layout({ children }) {
  const { user, logout } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [shopUrl, setShopUrl] = useState(null);
  const [commerceEnabled, setCommerceEnabled] = useState(false);

  // Storefront link (shown only when shop_url is set) and whether this build
  // carries the commerce surface (gates the Commerce nav section).
  useEffect(() => {
    let cancelled = false;
    fetchApi("/api/site/config")
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => {
        if (cancelled || !d) return;
        if (d.shop_url) setShopUrl(d.shop_url);
        setCommerceEnabled(!!d.commerce_enabled);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const isActive = (path) => location.pathname === path;

  function go(path) {
    navigate(path);
    setDrawerOpen(false);
  }

  // Keep the access-codes nav entry only when the feature flag enables it
  const showAccessCodes = user?.features?.access_codes_enabled !== false;

  return (
    <div className="layout">
      <header className="layout-header">
        <div className="header-content">
          <button
            type="button"
            className="layout-hamburger"
            aria-label="Open navigation"
            aria-expanded={drawerOpen}
            onClick={() => setDrawerOpen((v) => !v)}
          >
            <span aria-hidden="true">☰</span>
          </button>
          <h1 onClick={() => go("/dashboard")} className="site-title">
            Admin
          </h1>
          <div className="header-spacer" />
          <AccountMenu user={user} onLogout={logout} onNavigate={go} />
        </div>
      </header>

      <div className="layout-body">
        <aside
          className={`layout-sidebar ${drawerOpen ? "is-open" : ""}`}
          aria-label="Admin navigation"
        >
          <nav className="sidebar-nav">
            <SidebarLink
              path="/dashboard"
              isActive={isActive}
              onClick={go}
              label="Dashboard"
            />

            <SidebarSection label="Content" />
            <SidebarLink
              path="/moderation"
              isActive={isActive}
              onClick={go}
              label="Moderation"
            />

            <SidebarSection label="Users" />
            <SidebarLink
              path="/users"
              isActive={isActive}
              onClick={go}
              label="Users"
            />
            <SidebarLink
              path="/invite-codes"
              isActive={isActive}
              onClick={go}
              label="Invites"
            />

            <SidebarSection label="Data" />
            <SidebarLink
              path="/imports"
              isActive={isActive}
              onClick={go}
              label="Imports"
            />

            {commerceEnabled && (
              <>
                <SidebarSection label="Commerce" />
                <SidebarLink
                  path="/orders"
                  isActive={isActive}
                  onClick={go}
                  label="Orders"
                />
              </>
            )}

            <SidebarSection label="System" />
            {showAccessCodes && (
              <SidebarLink
                path="/access-codes"
                isActive={isActive}
                onClick={go}
                label="Access Codes"
              />
            )}
            <SidebarLink
              path="/access-logs"
              isActive={isActive}
              onClick={go}
              label="Access Logs"
            />
            <SidebarLink
              path="/settings"
              isActive={isActive}
              onClick={go}
              label="Settings"
            />
            <a
              href="/"
              className="sidebar-link"
              onClick={() => setDrawerOpen(false)}
            >
              View Site
            </a>
            {shopUrl && (
              <a
                href={shopUrl}
                className="sidebar-link"
                target="_blank"
                rel="noopener noreferrer"
                onClick={() => setDrawerOpen(false)}
              >
                Store
              </a>
            )}
          </nav>
        </aside>

        {drawerOpen && (
          <div
            className="sidebar-backdrop"
            onClick={() => setDrawerOpen(false)}
            aria-hidden="true"
          />
        )}

        <main className="layout-main">{children}</main>
      </div>
    </div>
  );
}

function SidebarSection({ label }) {
  return <div className="sidebar-section-label">{label}</div>;
}

function SidebarLink({ path, isActive, onClick, label }) {
  return (
    <button
      type="button"
      className={`sidebar-link ${isActive(path) ? "active" : ""}`}
      onClick={() => onClick(path)}
    >
      {label}
    </button>
  );
}

/// Avatar / email button on the right of the header. Click to open a
/// popover with View profile + Logout. Click-outside and Escape close it.
function AccountMenu({ user, onLogout, onNavigate }) {
  const [open, setOpen] = useState(false);
  const ref = useRef(null);

  useEffect(() => {
    if (!open) return undefined;
    function onDoc(e) {
      if (ref.current && !ref.current.contains(e.target)) setOpen(false);
    }
    function onKey(e) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const display = user?.handle || user?.email || "Account";
  const initials = computeInitials(user?.email || display);

  return (
    <div className="account-menu" ref={ref}>
      <button
        type="button"
        className="account-menu-trigger"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {user?.avatar_url ? (
          <img className="account-menu-avatar" src={user.avatar_url} alt="" />
        ) : (
          <span
            className="account-menu-avatar account-menu-initials"
            aria-hidden="true"
          >
            {initials}
          </span>
        )}
        <span className="account-menu-label">{display}</span>
        <span className="account-menu-chevron" aria-hidden="true">
          ▾
        </span>
      </button>
      {open && (
        <div className="account-menu-popover" role="menu">
          {user?.email && (
            <div className="account-menu-meta">
              <div className="account-menu-meta-name">{display}</div>
              <div className="account-menu-meta-email">{user.email}</div>
            </div>
          )}
          <div className="account-menu-divider" aria-hidden="true" />
          <button
            type="button"
            role="menuitem"
            className="account-menu-item"
            onClick={() => {
              setOpen(false);
              onNavigate("/profile");
            }}
          >
            View profile
          </button>
          <button
            type="button"
            role="menuitem"
            className="account-menu-item account-menu-signout"
            onClick={() => {
              setOpen(false);
              onLogout();
            }}
          >
            Logout
          </button>
        </div>
      )}
    </div>
  );
}

function computeInitials(value) {
  if (!value) return "?";
  const trimmed = String(value).trim();
  if (trimmed.includes("@")) return trimmed.slice(0, 2).toUpperCase();
  const parts = trimmed.split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]).join("").toUpperCase() || "?";
}

export default Layout;

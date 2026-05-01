import { useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { useSiteConfig } from "../contexts/SiteConfigContext";
import MobileDrawer from "./MobileDrawer";
import ThemePicker from "./ThemePicker";
import UserMenu from "./UserMenu";
import "./Layout.css";

/// Shared application shell for the social frontend.
///
/// Replaces the per-page `<header className="feed-header">` blocks that
/// duplicated logo, auth state, and back-to-feed links across every page.
/// Renders three regions:
///
///   <header>  : logo (link to /), inline nav (Feed / Compose),
///               ThemePicker, auth state. On narrow viewports nav links
///               and ThemePicker collapse into a hamburger drawer.
///   <main>    : a 12-col grid with optional left and right rail slots
///               surrounding the center content column. The grid resolves
///               to a single column when rail props are not supplied,
///               so today's UI looks identical to the pre-Layout version
///               while leaving room for Phase 8 / 9 to drop in nav rails
///               and activity panels.
///   <footer>  : ThemePicker is sufficient on the header; the footer is
///               minimal and may grow with cookie-consent re-open later.
///
/// Auth-aware affordances (Compose link, Sign in vs Sign out) live here
/// so the per-page logic doesn't have to repeat them. The fail-closed
/// gating from Phase 6 is preserved: the Compose link only renders when
/// `siteConfig.poster_posting_enabled === true` for poster users.
export default function Layout({ leftRail, rightRail, children }) {
  const { user, loading: authLoading, logout } = useAuth();
  const { config: site } = useSiteConfig();
  const location = useLocation();
  const [drawerOpen, setDrawerOpen] = useState(false);

  // Posters can be muted by an admin via `poster_posting_enabled`;
  // admins always retain access. Treat anything other than explicit true
  // as disabled (fail closed) — the site config fetch starts as null and
  // only populates on success.
  const canCompose =
    user &&
    (user.role === "administrator" ||
      (user.role === "poster" &&
        site !== null &&
        site.poster_posting_enabled === true));

  const navLinks = [
    { to: "/", label: "Feed" },
    canCompose ? { to: "/compose", label: "Compose" } : null,
    canCompose ? { to: "/compose-album", label: "New album" } : null,
  ].filter(Boolean);

  return (
    <div className="layout">
      <a className="layout-skip-link" href="#main-content">
        Skip to content
      </a>
      <header className="layout-header">
        <div className="layout-header-inner">
          <Link to="/" className="layout-logo" aria-label="Riposte Social home">
            Riposte Social
          </Link>
          <nav className="layout-nav" aria-label="Primary">
            {navLinks.map((l) => (
              <Link
                key={l.to}
                to={l.to}
                className={`layout-nav-link ${location.pathname === l.to ? "active" : ""}`}
              >
                {l.label}
              </Link>
            ))}
          </nav>
          <div className="layout-header-actions">
            <ThemePicker />
            {!authLoading &&
              (user ? (
                <UserMenu user={user} onSignOut={logout} />
              ) : (
                <Link to="/login" className="btn-primary layout-auth-btn">
                  Sign in
                </Link>
              ))}
            <button
              type="button"
              className="layout-hamburger"
              aria-label="Open menu"
              aria-expanded={drawerOpen}
              onClick={() => setDrawerOpen(true)}
            >
              <svg
                width="22"
                height="22"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                aria-hidden="true"
              >
                <line x1="4" y1="7" x2="20" y2="7" />
                <line x1="4" y1="12" x2="20" y2="12" />
                <line x1="4" y1="17" x2="20" y2="17" />
              </svg>
            </button>
          </div>
        </div>
      </header>

      <div className={`layout-grid ${leftRail ? "has-left" : ""} ${rightRail ? "has-right" : ""}`}>
        {leftRail && <aside className="layout-rail layout-rail-left">{leftRail}</aside>}
        <main id="main-content" className="layout-main">
          {children}
        </main>
        {rightRail && <aside className="layout-rail layout-rail-right">{rightRail}</aside>}
      </div>

      <MobileDrawer
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        navLinks={navLinks}
        user={user}
        onSignOut={logout}
      />
    </div>
  );
}

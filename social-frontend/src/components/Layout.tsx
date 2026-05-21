import React, { useEffect, useRef, useState } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../contexts/AuthContext";
import { useSiteConfig } from "../contexts/SiteConfigContext";
import BrowseRail from "./BrowseRail";
import LanguagePicker from "./LanguagePicker";
import LoadingBar from "./LoadingBar";
import MobileDrawer from "./MobileDrawer";
import ThemePicker from "./ThemePicker";
import UserMenu from "./UserMenu";
import { resetLoadCount } from "../utils/loadingState";
import "./Layout.css";

/// Shared application shell for the social frontend.
///
/// Renders three regions:
///
///   <header>  : logo (link to /), inline nav (Feed / Compose),
///               theme/language pickers, auth state. On narrow viewports
///               the nav links and pickers collapse into a hamburger
///               drawer.
///   <main>    : a 12-col grid with optional left and right rail slots
///               surrounding the center content column. The grid resolves
///               to a single column when rail props are not supplied, so
///               pages that opt out of rails render identically to a
///               single-column shell.
///   <footer>  : minimal placeholder for cookie-consent re-open, etc.
///
/// Auth-aware affordances (Compose link, Sign in vs the user menu) live
/// here so the per-page logic doesn't have to repeat them. The Compose
/// link is fail-closed: it only renders when
/// `siteConfig.poster_posting_enabled === true` for poster users (admins
/// always retain access).
interface LayoutProps {
  leftRail?: React.ReactNode;
  rightRail?: React.ReactNode;
  /// Optional explicit child content. When omitted (the normal case under
  /// React Router's layout-route pattern), the matched child route's
  /// element is rendered via `<Outlet />`. Keeping `children` available
  /// lets non-routed call sites embed Layout directly when useful.
  children?: React.ReactNode;
}

export default function Layout({ leftRail, rightRail, children }: LayoutProps) {
  const { user, loading: authLoading, logout } = useAuth();
  const { config: site } = useSiteConfig();
  const location = useLocation();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const { t } = useTranslation("common");

  // Reset the loading bucket on actual route change. Skip the initial
  // mount: at that point child effects (e.g. the page's data fetch) have
  // already fired their startLoad and a reset here would wipe their
  // counters mid-flight, leaving the bucket inconsistent when their
  // endLoads land.
  const previousPath = useRef<string | null>(null);
  useEffect(() => {
    if (previousPath.current !== null && previousPath.current !== location.pathname) {
      resetLoadCount();
    }
    previousPath.current = location.pathname;
  }, [location.pathname]);

  // Auto-mount the BrowseRail when the caller didn't pass an explicit
  // leftRail. Anonymous viewers also get the rail; categories are public
  // and albums respect server-side visibility.
  const effectiveLeftRail = leftRail !== undefined ? leftRail : <BrowseRail />;

  // Posters can be muted by an admin via `poster_posting_enabled`;
  // admins always retain access. Treat anything other than explicit true
  // as disabled (fail closed); the site config fetch starts as null and
  // only populates on success.
  const canCompose =
    user &&
    (user.role === "administrator" ||
      (user.role === "poster" &&
        site !== null &&
        site.poster_posting_enabled === true));

  const navLinks = [
    { to: "/", label: t("nav.feed") },
    canCompose ? { to: "/compose", label: t("nav.compose") } : null,
    canCompose ? { to: "/compose-album", label: t("nav.newAlbum") } : null,
  ].filter(Boolean);

  return (
    <div className="layout">
      <LoadingBar />
      <a className="layout-skip-link" href="#main-content">
        {t("skipToContent")}
      </a>
      <header className="layout-header">
        <div className="layout-header-inner">
          <Link to="/" className="layout-logo" aria-label={t("homeAria")}>
            {t("siteName")}
          </Link>
          <nav className="layout-nav" aria-label={t("primaryNav")}>
            {navLinks.map((l) => {
              const isActive = location.pathname === l.to;
              return (
                <Link
                  key={l.to}
                  to={l.to}
                  className={`layout-nav-link ${isActive ? "active" : ""}`}
                  aria-current={isActive ? "page" : undefined}
                >
                  {l.label}
                </Link>
              );
            })}
          </nav>
          <div className="layout-header-actions">
            <LanguagePicker />
            <ThemePicker />
            {!authLoading &&
              (user ? (
                <UserMenu user={user} onSignOut={logout} />
              ) : (
                <Link to="/login" className="btn-primary layout-auth-btn">
                  {t("auth.signIn")}
                </Link>
              ))}
            <button
              type="button"
              className="layout-hamburger"
              aria-label={t("openMenu")}
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

      <div className={`layout-grid ${effectiveLeftRail ? "has-left" : ""} ${rightRail ? "has-right" : ""}`}>
        {effectiveLeftRail && (
          <aside className="layout-rail layout-rail-left">{effectiveLeftRail}</aside>
        )}
        <main id="main-content" className="layout-main">
          {children ?? <Outlet />}
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

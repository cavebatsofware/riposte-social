import { useEffect } from "react";
import { Link, useLocation } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useFocusTrap } from "../utils/useFocusTrap";
import LanguagePicker from "./LanguagePicker";
import ThemePicker from "./ThemePicker";
import "./MobileDrawer.css";

/// Slide-in panel from the right edge for narrow viewports.
///
/// Rendered by `<Layout>` and toggled via the header's hamburger button.
/// Open state is owned by the parent so the close handler can come from
/// outside as well (e.g. a route change). Backdrop click and Escape
/// dismiss; focus is trapped inside the panel while open and restored to
/// the trigger when closed (via the shared `useFocusTrap` hook).
///
/// Closed-state always renders an empty placeholder so the markup stays
/// stable; the slide animation comes from a `data-open` attribute and a
/// CSS transition on transform/opacity.
export default function MobileDrawer({ open, onClose, navLinks, user, onSignOut }) {
  const { t } = useTranslation("common");
  const location = useLocation();
  const panelRef = useFocusTrap(open, { onEscape: onClose });

  // Lock body scroll while the drawer is open. Focus trap and Escape are
  // handled by the shared hook.
  useEffect(() => {
    if (!open) return undefined;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = "";
    };
  }, [open]);

  return (
    <div
      className="mobile-drawer"
      data-open={open ? "true" : "false"}
      aria-hidden={!open}
    >
      <div
        className="mobile-drawer-backdrop"
        onClick={onClose}
        aria-hidden="true"
      />
      <aside
        ref={panelRef}
        className="mobile-drawer-panel"
        role="dialog"
        aria-modal="true"
        aria-label={t("menuTitle")}
      >
        <div className="mobile-drawer-header">
          <span className="mobile-drawer-title">{t("menuTitle")}</span>
          <button
            type="button"
            className="mobile-drawer-close"
            aria-label={t("closeMenu")}
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <nav className="mobile-drawer-nav" aria-label={t("primaryNav")}>
          {navLinks.map((l) => {
            const isActive = location.pathname === l.to;
            return (
              <Link
                key={l.to}
                to={l.to}
                className="mobile-drawer-nav-link"
                aria-current={isActive ? "page" : undefined}
                onClick={onClose}
              >
                {l.label}
              </Link>
            );
          })}
          {user && user.handle && (
            <Link
              to={`/u/${user.handle}`}
              className="mobile-drawer-nav-link"
              aria-current={location.pathname === `/u/${user.handle}` ? "page" : undefined}
              onClick={onClose}
            >
              {t("userMenu.viewProfile")}
            </Link>
          )}
          {user && (
            <Link
              to="/settings/profile"
              className="mobile-drawer-nav-link"
              aria-current={location.pathname === "/settings/profile" ? "page" : undefined}
              onClick={onClose}
            >
              {t("userMenu.settings")}
            </Link>
          )}
        </nav>
        <div className="mobile-drawer-section">
          <LanguagePicker variant="inline" />
        </div>
        <div className="mobile-drawer-section">
          <ThemePicker variant="inline" />
        </div>
        <div className="mobile-drawer-section mobile-drawer-auth">
          {user ? (
            <button
              type="button"
              className="btn-secondary mobile-drawer-btn"
              onClick={() => {
                onSignOut();
                onClose();
              }}
            >
              {t("auth.signOut")}
            </button>
          ) : (
            <Link
              to="/login"
              className="btn-primary mobile-drawer-btn"
              onClick={onClose}
            >
              {t("auth.signIn")}
            </Link>
          )}
        </div>
      </aside>
    </div>
  );
}

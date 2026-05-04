import { useEffect, useRef } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import LanguagePicker from "./LanguagePicker";
import ThemePicker from "./ThemePicker";
import "./MobileDrawer.css";

/// Slide-in panel from the right edge for narrow viewports.
///
/// Rendered by `<Layout>` and toggled via the header's hamburger button.
/// Open state is owned by the parent so the close handler can come from
/// outside as well (e.g. a route change). Backdrop click and Escape
/// dismiss; focus is moved to the close button when the drawer opens
/// and a focus trap keeps Tab inside while open.
///
/// Closed-state always renders an empty placeholder so the markup stays
/// stable; the slide animation comes from a `data-open` attribute and a
/// CSS transition on transform/opacity.
export default function MobileDrawer({ open, onClose, navLinks, user, onSignOut }) {
  const panelRef = useRef(null);
  const closeBtnRef = useRef(null);
  const lastFocusRef = useRef(null);
  const { t } = useTranslation("common");

  // Close on Escape, restore prior focus on close, focus the close button on open.
  useEffect(() => {
    if (!open) return undefined;
    lastFocusRef.current = document.activeElement;
    closeBtnRef.current?.focus();
    function onKey(e) {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.key === "Tab" && panelRef.current) {
        // Minimal focus trap: cycle within the panel's focusable elements.
        const focusables = panelRef.current.querySelectorAll(
          'a, button, input, textarea, [tabindex]:not([tabindex="-1"])',
        );
        if (focusables.length === 0) return;
        const first = focusables[0];
        const last = focusables[focusables.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }
    document.addEventListener("keydown", onKey);
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", onKey);
      document.body.style.overflow = "";
      if (lastFocusRef.current && lastFocusRef.current.focus) {
        lastFocusRef.current.focus();
      }
    };
  }, [open, onClose]);

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
            ref={closeBtnRef}
            type="button"
            className="mobile-drawer-close"
            aria-label={t("closeMenu")}
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <nav className="mobile-drawer-nav" aria-label={t("primaryNav")}>
          {navLinks.map((l) => (
            <Link
              key={l.to}
              to={l.to}
              className="mobile-drawer-nav-link"
              onClick={onClose}
            >
              {l.label}
            </Link>
          ))}
          {user && user.handle && (
            <Link
              to={`/u/${user.handle}`}
              className="mobile-drawer-nav-link"
              onClick={onClose}
            >
              {t("userMenu.viewProfile")}
            </Link>
          )}
          {user && (
            <Link
              to="/settings/profile"
              className="mobile-drawer-nav-link"
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

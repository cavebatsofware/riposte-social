import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";

/// Avatar dropdown that replaces the bare Sign-out button in `<Layout>`.
///
/// Renders the viewer's avatar (or initials fallback) as a button. Clicking
/// it opens a small popover with profile / settings / sign-out entries.
/// Keyboard navigation: Tab cycles through items; Escape closes; click-
/// outside closes.
export default function UserMenu({ user, onSignOut }) {
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef(null);
  const { t } = useTranslation("common");

  useEffect(() => {
    if (!open) return undefined;
    function onDocClick(e) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target)) {
        setOpen(false);
      }
    }
    function onKey(e) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const display =
    user.display_name || user.handle || user.email || t("userMenu.fallbackName");
  const initials = computeInitials(display);
  const profileTo = user.handle ? `/u/${user.handle}` : null;

  return (
    <div className="user-menu" ref={wrapperRef}>
      <button
        type="button"
        className="user-menu-trigger"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={t("userMenu.openAria")}
        onClick={() => setOpen((v) => !v)}
      >
        {user.avatar_url ? (
          <img src={user.avatar_url} alt="" className="user-menu-avatar" />
        ) : (
          <span className="user-menu-avatar user-menu-initials" aria-hidden="true">
            {initials}
          </span>
        )}
      </button>

      {open && (
        <div className="user-menu-popover" role="menu">
          <div className="user-menu-meta">
            <div className="user-menu-name">{display}</div>
            {user.handle && (
              <div className="user-menu-handle">@{user.handle}</div>
            )}
          </div>
          <div className="user-menu-divider" aria-hidden="true" />
          {profileTo && (
            <Link
              to={profileTo}
              className="user-menu-item"
              role="menuitem"
              onClick={() => setOpen(false)}
            >
              {t("userMenu.viewProfile")}
            </Link>
          )}
          <Link
            to="/settings/profile"
            className="user-menu-item"
            role="menuitem"
            onClick={() => setOpen(false)}
          >
            {t("userMenu.settings")}
          </Link>
          <button
            type="button"
            className="user-menu-item user-menu-signout"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onSignOut();
            }}
          >
            {t("auth.signOut")}
          </button>
        </div>
      )}
    </div>
  );
}

function computeInitials(name) {
  if (!name) return "??";
  if (name.includes("@")) return name.slice(0, 2).toUpperCase();
  const parts = name.trim().split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]).join("").toUpperCase() || "??";
}

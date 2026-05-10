import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import useRovingFocus from "../utils/useRovingFocus";

/// Avatar dropdown that replaces the bare Sign-out button in `<Layout>`.
///
/// Renders the viewer's avatar (or initials fallback) as a button.
/// Clicking it opens a small popover with profile / settings / sign-out
/// entries. Keyboard navigation: Up/Down arrows + Home/End move between
/// items, Escape closes, click-outside closes, and focus leaving the
/// wrapper closes too so a Tab to the next picker's trigger doesn't
/// leave this menu open in the background. The trigger gets focus back
/// on Escape.
export default function UserMenu({ user, onSignOut }) {
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef(null);
  const popoverRef = useRef(null);
  const triggerRef = useRef(null);
  const { t } = useTranslation("common");

  useRovingFocus(popoverRef, open);

  useEffect(() => {
    if (!open) return undefined;
    function onDocClick(e) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target)) {
        setOpen(false);
      }
    }
    // Close when focus leaves the wrapper. Without this a keyboard user
    // can Tab from a menu item to an adjacent picker's trigger and open
    // the second one while this menu stays open, since mousedown never
    // fires. Mirrors the same listener PopoverPicker uses for the theme
    // and language pickers.
    function onFocusIn(e) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target)) {
        setOpen(false);
      }
    }
    function onKey(e) {
      if (e.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    }
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("focusin", onFocusIn);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("focusin", onFocusIn);
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
        ref={triggerRef}
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
        <div ref={popoverRef} className="user-menu-popover" role="menu">
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
          {user.role === "administrator" && (
            <a
              href="/admin/"
              className="user-menu-item"
              role="menuitem"
              onClick={() => setOpen(false)}
            >
              {t("userMenu.admin")}
            </a>
          )}
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

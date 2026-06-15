import { useEffect } from "react";
import { Link, useLocation } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { Drawer } from "@cavebatsofware/riposte-design-system/components";
import { LanguagePicker, ThemePicker } from "@cavebatsofware/riposte-pickers";
import { useSiteConfig } from "../contexts/SiteConfigContext";
import ComposeMenu from "./ComposeMenu";
import "./MobileDrawer.css";

/// Slide-in panel from the right edge for narrow viewports.
///
/// Rendered by `<Layout>` and toggled via the header's hamburger button. The
/// slide-in shell, focus trap, body-scroll lock, Escape / backdrop dismissal,
/// and the title + close header all come from the design-system `<Drawer>`;
/// this component supplies the Riposte navigation contents. Open state is owned
/// by the parent so the close handler can come from outside as well (e.g. a
/// route change).
export default function MobileDrawer({ open, onClose, navLinks, composeLinks = [], user, onSignOut }) {
  const { t } = useTranslation("common");
  const location = useLocation();
  const { config: site } = useSiteConfig();

  // Close the drawer on any route change. The link click handlers already call
  // onClose, but this also covers browser back/forward and imperative
  // navigation that bypasses the rendered links. Depends only on path/search so
  // the effect fires once per route change.
  useEffect(() => {
    if (open) onClose();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional: fires on route change only; open/onClose excluded to avoid triggering on every prop update
  }, [location.pathname, location.search]);

  return (
    <Drawer
      open={open}
      onClose={onClose}
      label={t("menuTitle")}
      title={t("menuTitle")}
      closeLabel={t("closeMenu")}
    >
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
        {site?.shop_url ? (
          <a
            className="mobile-drawer-nav-link"
            href={site.shop_url}
            target="_blank"
            rel="noopener noreferrer"
            onClick={onClose}
          >
            {t("nav.store")}
          </a>
        ) : null}
        {composeLinks.length > 0 && (
          <div className="mobile-drawer-section">
            <ComposeMenu variant="inline" links={composeLinks} />
          </div>
        )}
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
    </Drawer>
  );
}

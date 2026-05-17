/* global React */
const { useState, useEffect, useRef } = React;

/// Sticky header  wordmark, primary nav, language/theme pickers, user menu.
function Header({ route, onRoute, user, onSignOut }) {
  const links = [
    { id: "feed", label: "Feed" },
    user && { id: "compose", label: "Compose" },
    user && { id: "new-album", label: "New album" },
  ].filter(Boolean);

  return (
    <header className="layout-header">
      <div className="layout-header-inner">
        <button className="layout-logo" onClick={() => onRoute("feed")}>Riposte Social</button>
        <nav className="layout-nav" aria-label="Primary">
          {links.map((l) => (
            <button
              key={l.id}
              className={`layout-nav-link ${route === l.id ? "active" : ""}`}
              onClick={() => onRoute(l.id)}
              aria-current={route === l.id ? "page" : undefined}
            >
              {l.label}
            </button>
          ))}
        </nav>
        <div className="layout-header-actions">
          <LanguagePicker />
          <ThemePicker />
          {user ? (
            <UserMenu user={user} onSignOut={onSignOut} />
          ) : (
            <button className="btn-primary" onClick={() => onRoute("login")}>Sign in</button>
          )}
        </div>
      </div>
    </header>
  );
}

function LanguagePicker() {
  const [open, setOpen] = useState(false);
  const [lang, setLang] = useState("en");
  const ref = useRef(null);
  useOnOutsideClick(ref, () => setOpen(false));
  const languages = [
    { id: "en", name: "English" },
    { id: "de", name: "Deutsch" },
    { id: "es", name: "Español" },
    { id: "fr", name: "Français" },
    { id: "zh", name: "中文" },
  ];
  return (
    <div className="popover-wrap" ref={ref}>
      <button className="icon-btn" aria-label="Change language" onClick={() => setOpen((v) => !v)}>
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true">
          <circle cx="12" cy="12" r="9" />
          <path d="M3 12h18M12 3a14 14 0 010 18M12 3a14 14 0 000 18" />
        </svg>
      </button>
      {open && (
        <div className="popover">
          <div className="popover-title">Language</div>
          <div className="theme-swatches" role="radiogroup">
            {languages.map((l) => (
              <button
                key={l.id}
                role="radio"
                aria-checked={lang === l.id}
                className={`theme-swatch ${lang === l.id ? "active" : ""}`}
                onClick={() => { setLang(l.id); setOpen(false); }}
              >
                <span className="theme-swatch-label">{l.name}</span>
                {lang === l.id && <span className="theme-swatch-check" aria-hidden="true">✓</span>}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function UserMenu({ user, onSignOut }) {
  const [open, setOpen] = useState(false);
  const ref = useRef(null);
  useOnOutsideClick(ref, () => setOpen(false));
  return (
    <div className="popover-wrap" ref={ref}>
      <button className="user-trigger" aria-label="Open account menu" onClick={() => setOpen((v) => !v)}>
        <span className="user-initials">{user.initials}</span>
      </button>
      {open && (
        <div className="popover user-popover">
          <div className="user-meta">
            <div className="user-name">{user.displayName}</div>
            <div className="user-handle">@{user.handle}</div>
          </div>
          <div className="user-divider" />
          <button className="user-item">View profile</button>
          <button className="user-item">Settings</button>
          {user.role === "administrator" && <button className="user-item">Admin</button>}
          <div className="user-divider" />
          <button className="user-item" onClick={onSignOut}>Sign out</button>
        </div>
      )}
    </div>
  );
}

function useOnOutsideClick(ref, fn) {
  useEffect(() => {
    function onDown(e) {
      if (ref.current && !ref.current.contains(e.target)) fn();
    }
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [fn]);
}

Object.assign(window, { Header, useOnOutsideClick });

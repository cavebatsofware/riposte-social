import { useEffect, useRef, useState } from "react";
import { useTheme } from "../contexts/ThemeContext";

/// Theme picker for the social-frontend.
///
/// Two render modes:
/// - `variant="popover"` (default): a round icon button in the header
///   that opens a popover grid of (colorway × light/dark) swatches.
/// - `variant="inline"`: same grid rendered directly without a toggle,
///   intended for use inside the mobile drawer where vertical space is
///   abundant.
///
/// Phase 7d split each colorway into a light/dark pair. The grid renders
/// the user's current colorway and offers (a) a colorway switch and (b)
/// a light/dark toggle as two adjacent rows; the underlying state is a
/// single id of the form "<colorway>-<mode>" or just "<colorway>" for
/// historic light values (handled by `ThemeContext.setTheme`).
export default function ThemePicker({ variant = "popover" }) {
  const { theme, setTheme, colorways, mode, setMode } = useTheme();
  const [open, setOpen] = useState(false);
  const containerRef = useRef(null);

  useEffect(() => {
    if (variant !== "popover" || !open) return undefined;
    function handleClickOutside(e) {
      if (containerRef.current && !containerRef.current.contains(e.target)) {
        setOpen(false);
      }
    }
    function handleEsc(e) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleEsc);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEsc);
    };
  }, [open, variant]);

  // Today the theme id is "<colorway>" (light) or "<colorway>-dark". Derive
  // the current colorway from the resolved theme so the swatches show
  // the right active state regardless of which form is stored.
  const currentColorway = theme.endsWith("-dark") ? theme.slice(0, -"-dark".length) : theme;

  const grid = (
    <div className="theme-picker-grid">
      <div className="theme-picker-title">Theme</div>
      <div className="theme-swatches" role="radiogroup" aria-label="Colorway">
        {colorways.map((c) => {
          const active = c.id === currentColorway;
          return (
            <button
              key={c.id}
              type="button"
              role="radio"
              aria-checked={active}
              className={`theme-swatch ${active ? "active" : ""}`}
              onClick={() => {
                setTheme(`${c.id}${mode === "dark" ? "-dark" : ""}`);
                if (variant === "popover") setOpen(false);
              }}
            >
              <span
                className="theme-swatch-color"
                style={{ background: c.swatch }}
                aria-hidden="true"
              />
              <span className="theme-swatch-label">{c.label}</span>
              {active && (
                <span className="theme-swatch-check" aria-hidden="true">
                  ✓
                </span>
              )}
            </button>
          );
        })}
      </div>
      <div className="theme-mode-row" role="radiogroup" aria-label="Light or dark">
        {[
          { id: "light", label: "Light" },
          { id: "dark", label: "Dark" },
        ].map((m) => (
          <button
            key={m.id}
            type="button"
            role="radio"
            aria-checked={mode === m.id}
            className={`theme-mode-btn ${mode === m.id ? "active" : ""}`}
            onClick={() => setMode(m.id)}
          >
            {m.label}
          </button>
        ))}
      </div>
    </div>
  );

  if (variant === "inline") {
    return <div className="theme-picker-inline">{grid}</div>;
  }

  return (
    <div className="theme-picker" ref={containerRef}>
      {open && (
        <div className="theme-picker-popover" role="dialog" aria-label="Choose theme">
          {grid}
        </div>
      )}
      <button
        type="button"
        className="theme-picker-toggle"
        aria-label="Choose theme"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <circle cx="13.5" cy="6.5" r="0.5" fill="currentColor" />
          <circle cx="17.5" cy="10.5" r="0.5" fill="currentColor" />
          <circle cx="8.5" cy="7.5" r="0.5" fill="currentColor" />
          <circle cx="6.5" cy="12.5" r="0.5" fill="currentColor" />
          <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c1 0 1.5-.5 1.5-1.2 0-.4-.1-.7-.4-1-.2-.3-.4-.6-.4-1 0-.7.5-1.2 1.2-1.2H16c3.3 0 6-2.7 6-6 0-5-4.5-9-10-9z" />
        </svg>
      </button>
    </div>
  );
}

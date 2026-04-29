import { useEffect, useRef, useState } from "react";
import { useTheme } from "../contexts/ThemeContext";

/// Floating theme picker for the social-frontend. Mounted at app root so
/// every route gets it (Feed, Login, InviteAccept, etc.) without each
/// page having to render it. Click the round button to open a popover
/// listing the available palettes; click a swatch to switch. The choice
/// persists in localStorage for this origin.
export default function ThemePicker() {
  const { theme, setTheme, themes } = useTheme();
  const [open, setOpen] = useState(false);
  const containerRef = useRef(null);

  useEffect(() => {
    if (!open) return;
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
  }, [open]);

  return (
    <div className="theme-picker" ref={containerRef}>
      {open && (
        <div className="theme-picker-popover" role="dialog" aria-label="Choose theme">
          <div className="theme-picker-title">Theme</div>
          {themes.map((t) => {
            const active = t.id === theme;
            return (
              <button
                key={t.id}
                type="button"
                className={`theme-swatch ${active ? "active" : ""}`}
                onClick={() => {
                  setTheme(t.id);
                  setOpen(false);
                }}
              >
                <span
                  className="theme-swatch-color"
                  style={{ background: t.swatch }}
                />
                <span className="theme-swatch-label">{t.label}</span>
                {active && <span className="theme-swatch-check">✓</span>}
              </button>
            );
          })}
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

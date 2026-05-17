import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useTheme } from "../contexts/ThemeContext";
import PopoverPicker from "./PopoverPicker";

const ARROW_KEYS = ["ArrowDown", "ArrowUp", "ArrowRight", "ArrowLeft", "Home", "End"];

/// Resolve the next index for a roving radiogroup keystroke. ArrowDown /
/// ArrowRight advance, ArrowUp / ArrowLeft retreat, Home / End jump. The
/// list wraps at the ends.
function nextRadioIndex(key, currentIndex, length) {
  switch (key) {
    case "Home":
      return 0;
    case "End":
      return length - 1;
    case "ArrowDown":
    case "ArrowRight":
      return (currentIndex + 1) % length;
    case "ArrowUp":
    case "ArrowLeft":
      return (currentIndex - 1 + length) % length;
    default:
      return currentIndex;
  }
}

/// Theme picker for the social-frontend.
///
/// Two render modes:
/// - `variant="popover"` (default): a round icon button in the header
///   that opens a popover grid of (colorway × light/dark) swatches.
/// - `variant="inline"`: same grid rendered directly without a toggle,
///   intended for use inside the mobile drawer where vertical space is
///   abundant.
///
/// The grid renders the user's current colorway and offers (a) a
/// colorway switch and (b) a light/dark mode toggle as two adjacent
/// rows. The underlying state is a single id of the form
/// "<colorway>-<mode>" or just "<colorway>" for light-mode values
/// (both forms handled by `ThemeContext.setTheme`).
///
/// Each row is an ARIA radiogroup with roving tabindex: only the active
/// radio is in the tab order, and arrow keys move both focus and
/// selection across the row.
export default function ThemePicker({ variant = "popover" }) {
  const { theme, setTheme, colorways, mode, setMode } = useTheme();
  const [open, setOpen] = useState(false);
  const colorwayBtnRefs = useRef([]);
  const modeBtnRefs = useRef([]);
  const { t } = useTranslation("common");

  // Today the theme id is "<colorway>" (light) or "<colorway>-dark". Derive
  // the current colorway from the resolved theme so the swatches show
  // the right active state regardless of which form is stored.
  const currentColorway = theme.endsWith("-dark") ? theme.slice(0, -"-dark".length) : theme;

  const grid = (
    <div className="theme-picker-grid">
      <div className="theme-picker-title">{t("theme.title")}</div>
      <div
        className="theme-swatches"
        role="radiogroup"
        aria-label={t("theme.colorwayAria")}
      >
        {colorways.map((c, idx) => {
          const active = c.id === currentColorway;
          // Colorway display name comes from the catalog so each language
          // can localize the marketing-style names ("Forest & Cream"
          // etc.). Fall back to the array's hardcoded `c.label` if a key
          // is missing as defense-in-depth against a partial catalog.
          const label = t(`theme.colorways.${c.id}`, { defaultValue: c.label });
          return (
            <button
              key={c.id}
              ref={(el) => {
                colorwayBtnRefs.current[idx] = el;
              }}
              type="button"
              role="radio"
              aria-checked={active}
              tabIndex={active ? 0 : -1}
              className={`theme-swatch ${active ? "active" : ""}`}
              onClick={() => {
                setTheme(`${c.id}${mode === "dark" ? "-dark" : ""}`);
                if (variant === "popover") setOpen(false);
              }}
              onKeyDown={(e) => {
                if (!ARROW_KEYS.includes(e.key)) return;
                e.preventDefault();
                const next = nextRadioIndex(e.key, idx, colorways.length);
                const target = colorways[next];
                setTheme(`${target.id}${mode === "dark" ? "-dark" : ""}`);
                colorwayBtnRefs.current[next]?.focus();
              }}
            >
              <span
                className="theme-swatch-color"
                style={{ background: c.swatch }}
                aria-hidden="true"
              />
              <span className="theme-swatch-label">{label}</span>
              {active && (
                <span className="theme-swatch-check" aria-hidden="true">
                  ✓
                </span>
              )}
            </button>
          );
        })}
      </div>
      <div
        className="theme-mode-row"
        role="radiogroup"
        aria-label={t("theme.modeAria")}
      >
        {(() => {
          const modes = [
            { id: "light", label: t("theme.mode.light") },
            { id: "dark", label: t("theme.mode.dark") },
          ];
          return modes.map((m, idx) => (
            <button
              key={m.id}
              ref={(el) => {
                modeBtnRefs.current[idx] = el;
              }}
              type="button"
              role="radio"
              aria-checked={mode === m.id}
              tabIndex={mode === m.id ? 0 : -1}
              className={`theme-mode-btn ${mode === m.id ? "active" : ""}`}
              onClick={() => setMode(m.id)}
              onKeyDown={(e) => {
                if (!ARROW_KEYS.includes(e.key)) return;
                e.preventDefault();
                const next = nextRadioIndex(e.key, idx, modes.length);
                setMode(modes[next].id);
                modeBtnRefs.current[next]?.focus();
              }}
            >
              {m.label}
            </button>
          ));
        })()}
      </div>
    </div>
  );

  return (
    <PopoverPicker
      variant={variant}
      open={open}
      onOpenChange={setOpen}
      className="theme-picker"
      toggleAriaLabel={t("theme.toggleAria")}
      popoverAriaLabel={t("theme.dialogAria")}
      toggleIcon={
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
      }
    >
      {grid}
    </PopoverPicker>
  );
}

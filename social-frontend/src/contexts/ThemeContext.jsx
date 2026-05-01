import { createContext, useContext, useEffect, useState } from "react";

/// Available colorways. Each one has a light and a dark variant in
/// `index.css` under `[data-theme="<id>"]` and `[data-theme="<id>-dark"]`.
/// The theme id stored in localStorage is one of:
///   forest, forest-dark, warm, warm-dark, plum, plum-dark
/// The picker UI splits this into two axes: colorway (3 options) and
/// mode (light/dark).
export const COLORWAYS = [
  { id: "forest", label: "Forest & Cream", swatch: "#2d4a37" },
  { id: "warm", label: "Warm Editorial", swatch: "#1d3557" },
  { id: "plum", label: "Plum & Apricot", swatch: "#5b1f4d" },
  // Avernus & Clouds: opposite-pole light/dark. The picker swatch shows
  // both halves at once via a diagonal gradient so the dichotomy is
  // visible at a glance.
  {
    id: "avernus",
    label: "Avernus & Clouds",
    swatch: "linear-gradient(135deg, #4f6dab 50%, #d4452b 50%)",
  },
  { id: "mineral", label: "Rocks & Minerals", swatch: "#a05a2c" },
];

const STORAGE_KEY = "rs_theme_v1";
export const DEFAULT_COLORWAY = "forest";

const ThemeContext = createContext(null);

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used within ThemeProvider");
  }
  return ctx;
}

function isValidThemeId(id) {
  if (!id) return false;
  const colorway = id.endsWith("-dark") ? id.slice(0, -"-dark".length) : id;
  return COLORWAYS.some((c) => c.id === colorway);
}

function resolveInitialTheme() {
  let stored = null;
  try {
    stored = localStorage.getItem(STORAGE_KEY);
  } catch {
    // private mode without storage access; defaults stand
  }
  if (isValidThemeId(stored)) return stored;

  // No explicit user choice yet: honor the OS-level preference.
  let prefersDark = false;
  try {
    prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  } catch {
    // matchMedia unavailable (rare); default to light
  }
  return prefersDark ? `${DEFAULT_COLORWAY}-dark` : DEFAULT_COLORWAY;
}

/// Apply the theme to the document and persist the user's choice.
///
/// State of record is the resolved id (e.g. `forest-dark`); the picker UI
/// derives colorway and mode from it. Initial render uses
/// `${DEFAULT_COLORWAY}` so server-rendered markup is consistent with
/// the first paint; on mount we read localStorage and the OS preference
/// to find the right value, which may flip a light theme to dark briefly
/// (the FOUC is small and the cost of avoiding it is server-side
/// rendering we don't have).
///
/// Theme support is scoped to the social-frontend; the admin panel does
/// not consume this context.
export function ThemeProvider({ children }) {
  const [theme, setThemeState] = useState(DEFAULT_COLORWAY);

  // On mount: resolve from localStorage or system preference. Also subscribe
  // to OS preference changes so a fresh visitor (no explicit choice yet)
  // tracks their system theme — but stop tracking the moment they pick.
  useEffect(() => {
    const initial = resolveInitialTheme();
    setThemeState(initial);
    document.documentElement.dataset.theme = initial;

    let media;
    try {
      media = window.matchMedia("(prefers-color-scheme: dark)");
    } catch {
      return undefined;
    }
    function onChange(e) {
      let stored = null;
      try {
        stored = localStorage.getItem(STORAGE_KEY);
      } catch {
        // ignore
      }
      // User has made an explicit choice — don't override it.
      if (isValidThemeId(stored)) return;
      const next = e.matches ? `${DEFAULT_COLORWAY}-dark` : DEFAULT_COLORWAY;
      setThemeState(next);
      document.documentElement.dataset.theme = next;
    }
    media.addEventListener?.("change", onChange);
    return () => {
      media.removeEventListener?.("change", onChange);
    };
  }, []);

  function setTheme(id) {
    if (!isValidThemeId(id)) return;
    setThemeState(id);
    document.documentElement.dataset.theme = id;
    try {
      localStorage.setItem(STORAGE_KEY, id);
    } catch {
      // ignore; the in-memory state still reflects the choice for this session
    }
  }

  // Convenience: flip just the mode while keeping the colorway. The picker
  // exposes this as a separate row so users can change one without
  // accidentally re-picking the other.
  function setMode(nextMode) {
    if (nextMode !== "light" && nextMode !== "dark") return;
    const colorway = theme.endsWith("-dark") ? theme.slice(0, -"-dark".length) : theme;
    setTheme(nextMode === "dark" ? `${colorway}-dark` : colorway);
  }

  const mode = theme.endsWith("-dark") ? "dark" : "light";

  return (
    <ThemeContext.Provider
      value={{
        theme,
        setTheme,
        colorways: COLORWAYS,
        mode,
        setMode,
      }}
    >
      {children}
    </ThemeContext.Provider>
  );
}

// Backwards-compat re-export. The old `THEMES` array name is replaced by
// `COLORWAYS`; older imports of `THEMES` would break, but Phase 6 had no
// such imports outside the context itself.
export { COLORWAYS as THEMES };

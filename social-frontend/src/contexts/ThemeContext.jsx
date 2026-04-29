import { createContext, useContext, useEffect, useState } from "react";

/// Available palettes. The id is the value written to `document.documentElement.dataset.theme`,
/// matched by the `[data-theme="..."]` blocks in `index.css`. The swatch is
/// shown as a colored dot in the picker; pick a value that visually
/// represents the palette so the user can identify it without a label.
export const THEMES = [
  { id: "forest", label: "Forest & Cream", swatch: "#2d4a37" },
  { id: "warm", label: "Warm Editorial", swatch: "#1d3557" },
  { id: "plum", label: "Plum & Apricot", swatch: "#5b1f4d" },
];

const STORAGE_KEY = "rs_theme_v1";
export const DEFAULT_THEME = "forest";

const ThemeContext = createContext(null);

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used within ThemeProvider");
  }
  return ctx;
}

/// Apply the theme to the document and persist the user's choice.
///
/// Initial render uses DEFAULT_THEME so the markup is consistent with the
/// first paint before any client storage is read. After mount, localStorage
/// is read; if a valid id is found, switch to it (which updates the data-
/// attribute on the <html> element so the CSS variables re-resolve to the
/// chosen palette).
///
/// Theme support is scoped to the social-frontend; the admin panel does
/// not consume this context.
export function ThemeProvider({ children }) {
  const [theme, setThemeState] = useState(DEFAULT_THEME);

  useEffect(() => {
    let stored = null;
    try {
      stored = localStorage.getItem(STORAGE_KEY);
    } catch {
      // private mode without storage access; defaults stand
    }
    const next = THEMES.some((t) => t.id === stored) ? stored : DEFAULT_THEME;
    setThemeState(next);
    document.documentElement.dataset.theme = next;
  }, []);

  function setTheme(id) {
    if (!THEMES.some((t) => t.id === id)) return;
    setThemeState(id);
    document.documentElement.dataset.theme = id;
    try {
      localStorage.setItem(STORAGE_KEY, id);
    } catch {
      // ignore; the in-memory state still reflects the choice for this session
    }
  }

  return (
    <ThemeContext.Provider value={{ theme, setTheme, themes: THEMES }}>
      {children}
    </ThemeContext.Provider>
  );
}

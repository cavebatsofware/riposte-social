/* global React */
const { useState: useStateTP, useRef: useRefTP } = React;

/// Theme picker  colorway grid + light/dark radio row. Mirrors the
/// Riposte ThemePicker component (popover variant only here).
const COLORWAYS = [
  { id: "forest",   label: "Forest & Cream",         swatch: "#2d4a37" },
  { id: "warm",     label: "Warm Editorial",         swatch: "#1d3557" },
  { id: "plum",     label: "Plum & Apricot",         swatch: "#5b1f4d" },
  { id: "avernus",  label: "Avernus & Clouds",       swatch: "linear-gradient(135deg, #4f6dab 50%, #ed5e3a 50%)" },
  { id: "mineral",  label: "Rocks & Minerals",       swatch: "#8e4a1f" },
  { id: "daltonia", label: "Red-Green Accessible",   swatch: "#d4a334" },
  { id: "tritan",   label: "Blue-Yellow Accessible", swatch: "#c25d4a" },
  { id: "achroma",  label: "High Contrast",          swatch: "linear-gradient(135deg, #000 50%, #fff 50%)" },
];

function ThemePicker() {
  const [open, setOpen] = useStateTP(false);
  const [theme, setThemeState] = useStateTP(() => {
    const stored = localStorage.getItem("rs_theme_v1");
    return stored || "forest";
  });
  const ref = useRefTP(null);
  useOnOutsideClick(ref, () => setOpen(false));

  const colorway = theme.endsWith("-dark") ? theme.slice(0, -5) : theme;
  const mode = theme.endsWith("-dark") ? "dark" : "light";

  function apply(id) {
    setThemeState(id);
    document.documentElement.dataset.theme = id;
    try { localStorage.setItem("rs_theme_v1", id); } catch {}
  }

  function setColorway(id) { apply(mode === "dark" ? `${id}-dark` : id); }
  function setMode(m)      { apply(m === "dark" ? `${colorway}-dark` : colorway); }

  return (
    <div className="popover-wrap" ref={ref}>
      <button className="icon-btn" aria-label="Choose theme" onClick={() => setOpen((v) => !v)}>
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <circle cx="13.5" cy="6.5" r="0.5" fill="currentColor" />
          <circle cx="17.5" cy="10.5" r="0.5" fill="currentColor" />
          <circle cx="8.5" cy="7.5" r="0.5" fill="currentColor" />
          <circle cx="6.5" cy="12.5" r="0.5" fill="currentColor" />
          <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c1 0 1.5-.5 1.5-1.2 0-.4-.1-.7-.4-1-.2-.3-.4-.6-.4-1 0-.7.5-1.2 1.2-1.2H16c3.3 0 6-2.7 6-6 0-5-4.5-9-10-9z" />
        </svg>
      </button>
      {open && (
        <div className="popover">
          <div className="popover-title">Theme</div>
          <div className="theme-swatches" role="radiogroup" aria-label="Colorway">
            {COLORWAYS.map((c) => (
              <button
                key={c.id}
                role="radio"
                aria-checked={colorway === c.id}
                className={`theme-swatch ${colorway === c.id ? "active" : ""}`}
                onClick={() => setColorway(c.id)}
              >
                <span className="theme-swatch-color" style={{ background: c.swatch }} aria-hidden="true" />
                <span className="theme-swatch-label">{c.label}</span>
                {colorway === c.id && <span className="theme-swatch-check" aria-hidden="true">✓</span>}
              </button>
            ))}
          </div>
          <div className="theme-mode-row" role="radiogroup" aria-label="Light or dark">
            <button role="radio" aria-checked={mode === "light"} className={`theme-mode-btn ${mode === "light" ? "active" : ""}`} onClick={() => setMode("light")}>Light</button>
            <button role="radio" aria-checked={mode === "dark"}  className={`theme-mode-btn ${mode === "dark"  ? "active" : ""}`} onClick={() => setMode("dark")}>Dark</button>
          </div>
        </div>
      )}
    </div>
  );
}

Object.assign(window, { ThemePicker, COLORWAYS });

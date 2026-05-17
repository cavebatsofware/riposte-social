# Fonts

Reference snapshots of the four Google Fonts used by Riposte Social. **The active load path is the CDN** (`fonts.gstatic.com`)  these files exist in the folder for inspection / offline reference only.

| Family | Weight | Role |
|---|---|---|
| **Inter** | 400 / 500 / 600 / 700 | Body, UI, headings (Latin) |
| **JetBrains Mono** | 400 / 500 / 600 / 700 | Code, handles, technical labels (Latin + Latin-Ext) |
| **Noto Sans SC** | 400 / 500 / 600 / 700 | Simplified Chinese fallback |
| **Noto Color Emoji** | 400 | Reaction emoji (👍 ❤️ 😂 😮 😢 😡) |

## How they're loaded

A single `@import` at the top of [`../colors_and_type.css`](../colors_and_type.css) pulls all four from Google Fonts  same URL as production `social-frontend/index.html`:

```css
@import url("https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600;700&family=Noto+Sans+SC:wght@400;500;600;700&family=Noto+Color+Emoji&display=swap");
```

Each Google `@font-face` declaration carries its own `unicode-range`, so the browser fetches only the chunks whose code points the page actually uses. The CSS variables stack them in cascade order:

```css
--font-body: "Inter", "Noto Sans SC", "Noto Color Emoji", sans-serif;
--font-mono: "JetBrains Mono", "Noto Sans SC", "Noto Color Emoji", monospace;
```

## Snapshots in this folder

The WOFF2 files in this directory are reference copies of the chunks `fonts.gstatic.com` serves for the families above  useful for offline inspection, identifying which chunk covers which range, or air-gapped deployment. The active CSS does **not** reference them; the `@import` is the canonical load path.

```
UcC73FwrK3iLTeHuS_nVMrMxCp50SjIa1ZL7.woff2                                              · Inter (variable, Latin)
tDbv2o-flEEny0FZhsfKu5WU4zr3E_BX0PnT8RD8yKwBNntkaToggR7BYRbKPxDcwg.woff2                · JetBrains Mono (variable, Latin)
Yq6P-KqIXTD0t4D9z1ESnKM3-HpFabsE4tq3luCC7p-aXxcn.{7,9}.woff2                            · Noto Color Emoji (emoji ranges)
k3kXo84MPvpLmixcA63oeALhLOCT-…MNbE9VH8V.{5,87,88,101,109,116,118,119}.woff2             · Noto Sans SC (weight 400, 8 unicode chunks)
```

## Going fully offline

If the deployment can't reach `fonts.googleapis.com` / `fonts.gstatic.com`:

1. Replace the `@import` at the top of `colors_and_type.css` with explicit `@font-face` blocks pointing at the local files above.
2. Snapshot any additional weight or unicode-range chunks you need (the bundle here is sufficient for the chrome but doesn't cover every Noto Sans SC weight).
3. Keep `font-display: swap` on every block so the page renders before the font fetches.

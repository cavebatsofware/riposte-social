# Riposte Design System

Design tokens, colorways, and typography for the Riposte Social frontend. All tokens are CSS custom properties defined in `social-frontend/src/index.css`. Colorways are switched by setting `data-theme` on `<html>`.

---

## Colorways

Riposte ships 8 colorways (16 themes counting light and dark variants): 5 aesthetic and 3 designed for specific vision profiles. Every colorway ships a complete light and dark variant; the user selects at runtime with no page reload required.

Apply a theme: `<html data-theme="forest">` (default) or `<html data-theme="forest-dark">`.

### Aesthetic Colorways

#### Forest & Cream `forest` / `forest-dark`

Deep green primary, sand background, copper link underline. Default theme.

| Token | Light | Dark |
|---|---|---|
| Background | ![](https://placehold.co/14x14/f5efe4/f5efe4.png) `#f5efe4` | ![](https://placehold.co/14x14/14201a/14201a.png) `#14201a` |
| Surface | ![](https://placehold.co/14x14/fbf7ee/fbf7ee.png) `#fbf7ee` | ![](https://placehold.co/14x14/1a2922/1a2922.png) `#1a2922` |
| Primary | ![](https://placehold.co/14x14/2d4a37/2d4a37.png) `#2d4a37` | ![](https://placehold.co/14x14/7ec79a/7ec79a.png) `#7ec79a` |
| Text | ![](https://placehold.co/14x14/2b2a26/2b2a26.png) `#2b2a26` | ![](https://placehold.co/14x14/f1ead8/f1ead8.png) `#f1ead8` |
| Muted | ![](https://placehold.co/14x14/6c6453/6c6453.png) `#6c6453` | ![](https://placehold.co/14x14/a89e85/a89e85.png) `#a89e85` |

---

#### Warm Editorial `warm` / `warm-dark`

Navy primary, ivory base, terracotta accent.

| Token | Light | Dark |
|---|---|---|
| Background | ![](https://placehold.co/14x14/f7f1e8/f7f1e8.png) `#f7f1e8` | ![](https://placehold.co/14x14/131826/131826.png) `#131826` |
| Surface | ![](https://placehold.co/14x14/ffffff/ffffff.png) `#ffffff` | ![](https://placehold.co/14x14/1a2236/1a2236.png) `#1a2236` |
| Primary | ![](https://placehold.co/14x14/1d3557/1d3557.png) `#1d3557` | ![](https://placehold.co/14x14/8ab2dd/8ab2dd.png) `#8ab2dd` |
| Text | ![](https://placehold.co/14x14/1f2937/1f2937.png) `#1f2937` | ![](https://placehold.co/14x14/ece4d4/ece4d4.png) `#ece4d4` |
| Muted | ![](https://placehold.co/14x14/6e6452/6e6452.png) `#6e6452` | ![](https://placehold.co/14x14/a59c87/a59c87.png) `#a59c87` |

---

#### Plum & Apricot `plum` / `plum-dark`

Aubergine primary, cream surface, peach accents.

| Token | Light | Dark |
|---|---|---|
| Background | ![](https://placehold.co/14x14/fbf6f0/fbf6f0.png) `#fbf6f0` | ![](https://placehold.co/14x14/1c1220/1c1220.png) `#1c1220` |
| Surface | ![](https://placehold.co/14x14/ffffff/ffffff.png) `#ffffff` | ![](https://placehold.co/14x14/271a2c/271a2c.png) `#271a2c` |
| Primary | ![](https://placehold.co/14x14/5b1f4d/5b1f4d.png) `#5b1f4d` | ![](https://placehold.co/14x14/c884b6/c884b6.png) `#c884b6` |
| Text | ![](https://placehold.co/14x14/2a1a2e/2a1a2e.png) `#2a1a2e` | ![](https://placehold.co/14x14/f3e1d3/f3e1d3.png) `#f3e1d3` |
| Muted | ![](https://placehold.co/14x14/7a6c75/7a6c75.png) `#7a6c75` | ![](https://placehold.co/14x14/ad9a9f/ad9a9f.png) `#ad9a9f` |

---

#### Avernus & Clouds `avernus` / `avernus-dark`

Two personalities in one pair. Light variant ("Clouds"): sky-blue primary on cloud white. Dark variant ("Avernus"): ember orange on volcanic warm-black.

| Token | Light (Clouds) | Dark (Avernus) |
|---|---|---|
| Background | ![](https://placehold.co/14x14/f5f7fb/f5f7fb.png) `#f5f7fb` | ![](https://placehold.co/14x14/18120f/18120f.png) `#18120f` |
| Surface | ![](https://placehold.co/14x14/ffffff/ffffff.png) `#ffffff` | ![](https://placehold.co/14x14/221512/221512.png) `#221512` |
| Primary | ![](https://placehold.co/14x14/4f6dab/4f6dab.png) `#4f6dab` | ![](https://placehold.co/14x14/ed5e3a/ed5e3a.png) `#ed5e3a` |
| Text | ![](https://placehold.co/14x14/1a2438/1a2438.png) `#1a2438` | ![](https://placehold.co/14x14/f3e2cf/f3e2cf.png) `#f3e2cf` |
| Muted | ![](https://placehold.co/14x14/5e6878/5e6878.png) `#5e6878` | ![](https://placehold.co/14x14/b89381/b89381.png) `#b89381` |

---

#### Rocks & Minerals `mineral` / `mineral-dark`

Light variant ("Sandstone"): warm limestone, copper primary. Dark variant ("Granite"): cool charcoal, amber-copper primary.

| Token | Light (Sandstone) | Dark (Granite) |
|---|---|---|
| Background | ![](https://placehold.co/14x14/f0ebe1/f0ebe1.png) `#f0ebe1` | ![](https://placehold.co/14x14/1c1c1f/1c1c1f.png) `#1c1c1f` |
| Surface | ![](https://placehold.co/14x14/f8f5ed/f8f5ed.png) `#f8f5ed` | ![](https://placehold.co/14x14/25252a/25252a.png) `#25252a` |
| Primary | ![](https://placehold.co/14x14/8e4a1f/8e4a1f.png) `#8e4a1f` | ![](https://placehold.co/14x14/d6884a/d6884a.png) `#d6884a` |
| Text | ![](https://placehold.co/14x14/2d2925/2d2925.png) `#2d2925` | ![](https://placehold.co/14x14/ebe2cf/ebe2cf.png) `#ebe2cf` |
| Muted | ![](https://placehold.co/14x14/6c6453/6c6453.png) `#6c6453` | ![](https://placehold.co/14x14/9b9281/9b9281.png) `#9b9281` |

---

### Accessibility Colorways

These are designed *for* specific vision profiles, not merely compatible with them. Each replaces any color channel that a given condition collapses with a perceptually distinct alternative.

#### Daltonia `daltonia` / `daltonia-dark`

Red-Green CVD (deuteranopia / protanopia). Prussian blue primary on parchment (light); golden amber primary on deep navy (dark). No red or green in interactive or status roles.

| Token | Light | Dark |
|---|---|---|
| Background | ![](https://placehold.co/14x14/f5ecd5/f5ecd5.png) `#f5ecd5` | ![](https://placehold.co/14x14/0a1428/0a1428.png) `#0a1428` |
| Surface | ![](https://placehold.co/14x14/fbf6e6/fbf6e6.png) `#fbf6e6` | ![](https://placehold.co/14x14/142440/142440.png) `#142440` |
| Primary | ![](https://placehold.co/14x14/1c4a8c/1c4a8c.png) `#1c4a8c` | ![](https://placehold.co/14x14/e8b03a/e8b03a.png) `#e8b03a` |
| Text | ![](https://placehold.co/14x14/0e1a30/0e1a30.png) `#0e1a30` | ![](https://placehold.co/14x14/f0e8d0/f0e8d0.png) `#f0e8d0` |
| Muted | ![](https://placehold.co/14x14/4d5670/4d5670.png) `#4d5670` | ![](https://placehold.co/14x14/a89a7a/a89a7a.png) `#a89a7a` |

---

#### Tritan `tritan` / `tritan-dark`

Blue-Yellow CVD (tritanopia). Forest green primary on warm stone (light); terracotta-red primary on deep forest (dark). No blue or yellow in interactive or status roles.

| Token | Light | Dark |
|---|---|---|
| Background | ![](https://placehold.co/14x14/e6e8df/e6e8df.png) `#e6e8df` | ![](https://placehold.co/14x14/0d1a12/0d1a12.png) `#0d1a12` |
| Surface | ![](https://placehold.co/14x14/f0f2e8/f0f2e8.png) `#f0f2e8` | ![](https://placehold.co/14x14/152a1c/152a1c.png) `#152a1c` |
| Primary | ![](https://placehold.co/14x14/2c5538/2c5538.png) `#2c5538` | ![](https://placehold.co/14x14/dd7866/dd7866.png) `#dd7866` |
| Text | ![](https://placehold.co/14x14/1a2818/1a2818.png) `#1a2818` | ![](https://placehold.co/14x14/f0e8d0/f0e8d0.png) `#f0e8d0` |
| Muted | ![](https://placehold.co/14x14/525c44/525c44.png) `#525c44` | ![](https://placehold.co/14x14/8c9c70/8c9c70.png) `#8c9c70` |

---

#### Achroma `achroma` / `achroma-dark`

High contrast (achromatopsia / low vision). Pure black on white (light); pure white on black (dark). Borders are load-bearing; no information is conveyed by color alone.

| Token | Light | Dark |
|---|---|---|
| Background | ![](https://placehold.co/14x14/ffffff/ffffff.png) `#ffffff` | ![](https://placehold.co/14x14/000000/000000.png) `#000000` |
| Surface | ![](https://placehold.co/14x14/ffffff/ffffff.png) `#ffffff` | ![](https://placehold.co/14x14/0a0a0a/0a0a0a.png) `#0a0a0a` |
| Primary | ![](https://placehold.co/14x14/0000aa/0000aa.png) `#0000aa` | ![](https://placehold.co/14x14/66aaff/66aaff.png) `#66aaff` |
| Text | ![](https://placehold.co/14x14/000000/000000.png) `#000000` | ![](https://placehold.co/14x14/ffffff/ffffff.png) `#ffffff` |
| Muted | ![](https://placehold.co/14x14/404040/404040.png) `#404040` | ![](https://placehold.co/14x14/cccccc/cccccc.png) `#cccccc` |

---

## Typography

Fonts are loaded from Google Fonts. Each `@font-face` carries its own `unicode-range`, so the browser fetches only what the page uses and selects the right family per code-point.

| Stack | Families | Role |
|---|---|---|
| `--font-body` | Inter, Noto Sans SC, Noto Color Emoji, sans-serif | All UI text; Noto Sans SC covers Simplified Chinese; Noto Color Emoji covers emoji |
| `--font-mono` | JetBrains Mono, Noto Sans SC, Noto Color Emoji, monospace | Code, handles, timestamps |

### Type Scale

| Token | Value | Usage |
|---|---|---|
| `--font-size-xs` | 12px | Meta, captions, eyebrows |
| `--font-size-sm` | 14px | Secondary text, controls, chrome |
| `--font-size-base` | 16px | Body |
| `--font-size-lg` | 18px | Emphasized body, h3 |
| `--font-size-xl` | 20px | h2, logo wordmark |
| `--font-size-2xl` | 24px | Modal headings |
| `--font-size-3xl` | 32px | Feed h1, page hero |

---

## Spacing

4px grid. No half-steps.

| Token | Value |
|---|---|
| `--spacing-1` | 4px |
| `--spacing-2` | 8px |
| `--spacing-3` | 12px |
| `--spacing-4` | 16px |
| `--spacing-6` | 24px |
| `--spacing-8` | 32px |
| `--spacing-12` | 48px |

---

## Radius and Shadow

Small and restrained. No fully-pillowed corners except chips and reaction badges.

| Token | Value | Usage |
|---|---|---|
| `--radius-sm` | 4px | Inputs, search, inline code |
| `--radius-md` | 6px | Buttons, alerts, popovers |
| `--radius-lg` | 10px | Cards, modals, auth shell |
| `--radius-full` | 999px | Pills, avatars, reaction badges |
| `--shadow-card` | `0 1px 0 rgba(0,0,0,0.04)` | Feed cards |
| `--shadow-popover` | `0 8px 24px -8px rgba(0,0,0,0.18)` | Dropdowns, floating panels |

---

## Design Handoffs

Design references live alongside the issues they address and are not committed to the repo. The source of truth for implemented behavior is always the component source and the Cypress suite.

### Issue #39: Lightbox redesign

**Status:** ready for implementation.

Redesigns `MediaLightbox` so the image fills the viewport with a 24px peek of the engagement panel at the bottom as a scroll affordance. Adds a "View full size" link in the top chrome that opens the media at native resolution in a new tab.

Key layout change: replaces the `max-height: 70vh` image constraint with a single scroll column where the image frame is `min-height: calc(100vh - 24px)` and the engagement panel flows directly below it.

Target files:
- `social-frontend/src/components/MediaLightbox.jsx`
- `social-frontend/src/components/MediaLightbox.css`
- `social-frontend/public/locales/{en,de,es,fr,zh}/browse.json` (new key: `lightbox.viewFullSize`)

Full spec with annotated HTML prototype available in the Claude Design handoff for [issue #39](https://github.com/cavebatsofware/riposte-social/issues/39).

---

## Contributing

- All new components must use tokens from this system; no hardcoded hex values except in the lightbox dark chrome (`rgba(0,0,0,0.92)` and white-alpha overlays), which is intentionally theme-independent.
- New colorways must pass WCAG 2.1 AA contrast (4.5:1 for normal text, 3:1 for large text and UI components) verified with `make cypress-a11y`.
- The accessibility colorways (`daltonia`, `tritan`, `achroma`) must continue to pass simulation testing for their respective vision profiles.

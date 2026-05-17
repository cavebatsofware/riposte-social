# Riposte Social  Design System

A design system distilled from the [**riposte-social**](https://github.com/cavebatsofware/riposte-social) codebase  a self-hosted, invite-only social network for family and close friends. The product positions itself with one sentence:

> Own your posts, own your timeline, own your guest list.

Riposte is built by Grant DeFayette / cavebatsofware. It's Rust + Axum on the backend, React 19 on the frontend, and ships as a single Docker image you host yourself. The MVP is in active development  feature complete on auth/posting/reactions/albums; not yet production-ready.

This system covers **the social-frontend** (the public-facing product viewers and members use). The admin panel is a separate React SPA that does not consume these tokens.

---

## Sources

- **GitHub:** [`cavebatsofware/riposte-social`](https://github.com/cavebatsofware/riposte-social) (default branch `main`, commit `c1e378e` at the time of this snapshot)
- Imported under [`_source/`](./_source/) for offline reference  the live codebase is the source of truth if anything drifts.
- Live product screenshots from the maintainer are in [`_source/screenshot-feed.png`](./_source/screenshot-feed.png) and [`_source/screenshot-lightbox.png`](./_source/screenshot-lightbox.png).

If you want to dig deeper than this system covers, browse the repo  it's commented thoroughly, and most components carry a docblock that explains both the *what* and the *why*.

---

## Index  what's in this folder

| Path | What it is |
|---|---|
| [`README.md`](./README.md) | This file. |
| [`SKILL.md`](./SKILL.md) | Front-matter manifest so Claude Code can mount this as a skill. |
| [`colors_and_type.css`](./colors_and_type.css) | All color, type, spacing, radius, and shadow tokens. Drop-in. |
| [`fonts/`](./fonts/) | Self-hosted WOFF2 files for Inter, JetBrains Mono, and Noto Sans SC. Noto Color Emoji is intentionally absent  see caveats. |
| [`assets/`](./assets/) | Brand mark (cavebatsofware org logo). Riposte itself is wordmark-only. |
| [`preview/`](./preview/) | Per-token preview cards (rendered in the Design System tab). |
| [`ui_kits/riposte/`](./ui_kits/riposte/) | The single product UI kit: header, feed, post cards, lightbox, login. |
| [`_source/`](./_source/) | Imported social-frontend source for offline reference. |

There's only one product in scope: the **social-frontend**. The repo also contains a separate admin SPA (under `admin-frontend/`) and a generic template-derived `landing.html`  both intentionally out of scope here, since they don't share design language with the social product.

---

## Product context

Riposte is a **closed, invite-only network**. Admins issue invite codes; invitees see a welcome splash; new accounts can be poster or commenter. Anonymous visitors see a public feed if enabled.

Surfaces in scope:

- **Feed** (`/`)  chronological list of posts with author, body (markdown), attached photos/video, reactions, top comments, and a per-post visibility badge.
- **Browse rail**  Categories, Albums, People accordion groups in the left rail.
- **Post permalink** (`/post/:id`)  same card, full comment thread below.
- **Media lightbox**  fullscreen image/video viewer with its own reaction + comment panel.
- **Compose**  markdown editor with attachments and a Visibility chooser.
- **Login + Invite splash**  both password and OIDC modes; invite splash modal over the public feed.
- **Profile / People / Albums / Categories**  discovery and management pages.

---

## Content fundamentals

The product copy reads like an opinionated indie dev wrote it  warm where it can be, technical where it has to be. Specifics:

**Voice and tone**
- **Direct, plain, slightly editorial.** "Riposte Social is invite-only." "If you don't have an account yet, ask the administrator for an invite link."
- Addresses the reader as **"you"**; the maintainer never refers to themselves with "I/we" in user-facing copy.
- Confident in defaults, transparent about state: "Saving will publish immediately." "Posting has been temporarily disabled by an administrator. Existing posts are unaffected."
- Reassuring on privacy without being preachy: "We do not use tracking, analytics, or advertising cookies."

**Casing**
- **Sentence case** for headings, labels, and CTAs ("Sign in", "Accept invite", "Add a comment"). Never Title Case.
- **UPPERCASE with letter-spacing** is reserved for visibility/category pill badges ("PUBLIC", "COMMENTERS", "POSTERS") and the tiny eyebrow above pickers ("Theme", "Language").
- Site name uses internal capitals: "**Riposte Social**". The wordmark is the only logo  there's no graphical mark.

**Punctuation and microcopy**
- En dash for em-dash-style asides (`  `), ASCII arrows in nav links (`Show all categories →`, `← Back to feed`), and ellipsis as a single character (`Loading…`, `Saving…`, `Search posts…`).
- Confirmation copy is blunt and consequence-first: "Delete this post? This can't be undone."
- Errors are short, neutral, and don't blame: "Failed to load feed", "Couldn't update follow".

**Emoji**
- Used **functionally**, not decoratively. Six reaction glyphs are the entire emoji vocabulary in the chrome: 👍 Like · ❤️ Love · 😂 Haha · 😮 Wow · 😢 Sad · 😡 Angry. (Mirrors the Facebook-6 set, ordered exactly that way; the server keeps the allowlist.)
- A Noto Color Emoji webfont is loaded so reaction glyphs render identically across platforms.
- Emoji is **not** used in headings, labels, marketing copy, or cards. No 🎉 in empty states.

**Vibe**
- Closer in feel to a personal blog than a platform  small, warm, deliberate. Cream-and-forest by default; the dark mode is a quiet "deep forest at night" rather than a corporate slate-black. Eight curated colorways (including three accessibility-engineered themes) speak to a maintainer who cares about both aesthetics and inclusion.
- Translations exist for English, German, Spanish, French, and Simplified Chinese. The frontend ships `i18next` and a Noto Sans SC stack so CJK renders correctly without server work.

---

## Visual foundations

### Colorways

Riposte ships **eight named colorways**, each with a light and dark variant  sixteen total themes selected from a single picker. The default is `forest` (Forest & Cream).

| ID | Name | Identity |
|---|---|---|
| `forest` | **Forest & Cream** *(default)* | Deep green primary on warm sand; copper underlines. |
| `warm` | **Warm Editorial** | Prussian-navy primary on ivory; terracotta accents. |
| `plum` | **Plum & Apricot** | Aubergine primary on cream; peach badges. |
| `avernus` | **Avernus & Clouds** | Opposite-pole dichotomy: ethereal sky-blue on cloud-white light / volcanic ember on warm-black dark. |
| `mineral` | **Rocks & Minerals** | Earthy without being green  sandstone & limestone light, granite & amber dark. |
| `daltonia` | **Red-Green Accessible** | Designed FOR R-G CVD: parchment + prussian blue + golden amber. |
| `tritan` | **Blue-Yellow Accessible** | Designed FOR B-Y CVD: neutral stone + forest primary + distinct red/green/brown badges. |
| `achroma` | **High Contrast** | Pure B&W with a single saturated accent; borders are load-bearing. |

The accessibility colorways aren't filters over the aesthetic ones  they're standalone palettes engineered against simulated color-vision so every text and link surface clears WCAG 2.1 AA under the targeted deficiency.

All tokens land on the document via a single attribute on `<html>`:

```html
<html data-theme="forest">      <!-- light -->
<html data-theme="forest-dark"> <!-- dark companion -->
```

See [`colors_and_type.css`](./colors_and_type.css) for the full token list.

### Type

Three superfamilies, picked per Unicode range via the order in the stack:

- **`Inter`**  body, headings, UI. Weights 400 / 500 / 600 / 700.
- **`JetBrains Mono`**  code, handles, technical labels. Weights 400 / 500 / 600 / 700.
- **`Noto Sans SC`**  Simplified Chinese cascade. Weights 400 / 500 / 600 / 700.
- **`Noto Color Emoji`**  emoji cascade (reactions only).

All four are loaded from the Google Fonts CDN (`fonts.gstatic.com`) via a single `@import` in [`colors_and_type.css`](./colors_and_type.css)  same URL as production. Local WOFF2 snapshots of three of the four families live in [`fonts/`](./fonts/) for offline reference; see [`fonts/README.md`](./fonts/README.md).

Type scale (`1rem = 16px`):

| Token | Px | Used for |
|---|---|---|
| `--font-size-xs` | 12 | Eyebrow labels, captions, "edited" timestamps |
| `--font-size-sm` | 14 | Meta lines, buttons, search inputs, comments |
| `--font-size-base` | 16 | Body, post bodies, form inputs |
| `--font-size-lg` | 18 | Section headers, modal sub-headings, h3 |
| `--font-size-xl` | 20 | Site wordmark, h2 |
| `--font-size-2xl` | 24 | Modal h2 |
| `--font-size-3xl` | 32 | Page hero |

Body line-height is `1.55` baseline / `1.65` for post bodies (which use `text-wrap: pretty`-equivalent `overflow-wrap: anywhere` to keep tracking URLs from overflowing). Headings use `letter-spacing: -0.01em`.

### Spacing

A pure 4px grid via CSS custom properties: `--spacing-1` through `--spacing-12` (4, 8, 12, 16, 24, 32, 48). No T-shirt sizes  just the number.

### Radius

Restrained and consistent:

- `--radius-sm` 4px  inputs, code chips, search fields, small chips
- `--radius-md` 6px  buttons, alerts, popovers, post media
- `--radius-lg` 10px  cards, modals, auth shell
- `--radius-full` 999px  pill badges and avatar bubbles only

### Shadow

Two shadow tokens, both restrained  the system reads as editorial, not floaty:

- `--shadow-card: 0 1px 0 rgba(0, 0, 0, 0.04)`  a single hairline under a card, never a glow.
- `--shadow-popover: 0 8px 24px -8px rgba(0, 0, 0, 0.18)`  popovers, the user menu, the language/theme dropdowns.

Cards rely on a 1px border + the surface fill for depth, not shadow.

### Backgrounds

- **No imagery**, no gradients, no patterns. The body is a flat `--color-bg` fill.
- The only "media" the chrome owns is the **avatar bubble** (initials on a primary-color fill, or a circular-cropped uploaded image) and **post media** (object-fit cover on a `--color-media-bg` placeholder).
- No hero illustrations, no decorative dividers.

### Animation

Sparse and short  the system is not motion-driven.

- Theme transitions: `transition: background-color 0.2s ease, color 0.2s ease` on `<body>` only.
- Buttons / links: `0.15s` cross-fade on `background-color`, `border-color`, `color`.
- Reaction picker buttons scale `1.15` on hover with a `0.1s transform`.
- Tooltips show with a `0.15s opacity` and a `0.2s linger` so a small mouse-out doesn't snap them away.
- No bounces, no springs, no enter/exit animations on lists or routes.

### Hover, press, focus

- **Hover** on neutral controls: `background-color: var(--color-overlay)`  a tinted, theme-derived translucent fill (forest = `rgba(45, 74, 55, 0.55)` light / `rgba(126, 199, 154, 0.18)` dark, etc.). This is the dominant interactive affordance.
- **Hover** on primary buttons: swap to `--color-primary-hover` (a darker / lighter sibling of `--color-primary`).
- **Hover** on badges and chips: `filter: brightness(1.08)`  a uniform 8% lift across colors.
- **Press** is mostly visual = hover state; there's no shrink / scale.
- **Focus**: `outline: 2px solid var(--color-primary); outline-offset: 2px` on every interactive role via `:focus-visible`. The system explicitly enumerates ARIA interactive roles (`role="menuitem"`, `role="tab"`, etc.) in the focus selector so non-native focusables also get the ring. A "Skip to content" link is the first focusable element.
- **Disabled**: `opacity: 0.6; cursor: not-allowed` (or `cursor: progress` on async-pending states).

### Borders and dividers

- `--color-border` is the canonical card / input border, 1px solid, always present.
- `--color-divider` is the lighter inner separator inside cards (between body and actions, between actions and inline comments). Slightly muted relative to the border.
- The `achroma` (High Contrast) theme makes the border pure black/white and load-bearing  every other theme treats the border as a secondary affordance.

### Transparency and blur

- The chrome avoids backdrop-blur entirely.
- The two places transparency shows up: the **post-body clamp gradient** (a `linear-gradient(to bottom, rgba(0,0,0,0), var(--color-surface))` fade at the bottom of overflowing posts), and the **lightbox dim overlay** (`var(--color-overlay)`).

### Imagery

When user media renders:

- Photos and videos are `object-fit: cover` inside a fixed aspect-ratio frame (1:1 in 2-up grids, max-height: 480px for single).
- Videos in feed show a static thumbnail with a centered ▶ play badge; the lightbox is where playback happens.
- The **media lightbox** is the heaviest visual moment in the product  full-bleed image fills the viewport, a dim overlay obscures the page, and a small "engagement panel" peeks ~24px from the bottom edge as a scrollable affordance for reactions + comments. Native-resolution view is intentionally not exposed.

### Cards

Cards are the dominant container shape. Pattern:

```
background: var(--color-surface)
border:     1px solid var(--color-border)
border-radius: var(--radius-lg)        /* 10px */
padding:    var(--spacing-6)           /* 24px */
box-shadow: 0 1px 0 rgba(0,0,0,0.04)   /* hairline */
```

- Cards never get rounded-corners with a colored left-border accent.
- Cards don't lift on hover (the feed-card wrapper makes the whole card click-through to the permalink, but visually nothing moves).

### Layout

- Max body width: 1400px.
- Header is sticky, single-line (logo · nav · pickers · auth).
- Main column maxes at 840px (post readability cap); the rail is 240–260px; the right rail (when used) is 320px.
- Below 1024px the rail collapses into the column flow above main; the nav and theme picker hide into a hamburger drawer.
- Below 600px the outer gutter tightens to `var(--spacing-3)`.

---

## Iconography

Riposte has **almost no icon system**. The product is wordmark-and-text first, with a small set of hand-rolled glyphs and SVGs for the few moments an icon is needed.

**What you'll see in the chrome:**

| Where | What | Source |
|---|---|---|
| Site logo | "**Riposte Social**" wordmark | Plain text, `font-weight: 600`, `letter-spacing: -0.01em`. There is no graphical logo. |
| Reactions (6) | 👍 ❤️ 😂 😮 😢 😡 | Unicode emoji rendered via the Noto Color Emoji webfont. |
| Hamburger | three lines | Inline SVG, 22×22, `stroke="currentColor"`, `stroke-width=2`, `stroke-linecap="round"`. Lucide-adjacent. |
| Theme picker toggle | sparkly moon | Inline SVG, 20×20, `stroke-width=1.8`, `stroke-linecap="round"`. Hand-drawn. |
| Language picker toggle | globe | Inline SVG, same proportions. Hand-drawn. |
| Accordion chevron | `▾` open / `▸` closed | Unicode glyphs, sized `12px`. |
| Video play badge | `▶` | Unicode glyph in a 56×56 dark circular overlay. |
| Clear / close | `×` | Unicode glyph in a circular button. |
| Active radio / selected | `✓` | Unicode glyph. |
| Nav arrows in copy | `→` and `←` | ASCII arrows in user-facing strings (`Show all categories →`, `← Back to feed`). |
| Visibility menu chevron | `▾` | Unicode glyph, suppressed letter-spacing inline. |

**What there isn't:**

- No icon font. No SVG sprite. No Lucide / Heroicons / Feather import.
- No iconography on nav links, buttons, or empty states.
- No decorative emoji in headings or marketing copy.

**Brand mark for the maintainer (not the product):** the [cavebatsofware org logo](./assets/cavebatsofware-mark.png)  a neon-pink-and-cyan pixel bat on a circuit-board halo  is referenced by `landing.html` (a template-derived placeholder, not the social product) and as the GitHub avatar. It is the maintainer's identity, not Riposte's. Don't drop it onto Riposte UI.

**If you need an icon Riposte doesn't have:** match the existing hand-rolled inline SVG style  `stroke="currentColor"`, `stroke-width: 1.8–2`, `stroke-linecap="round"`, no fill, 20–22px box. The closest matching library is **Lucide** (same stroke conventions). Substituting a Lucide glyph as a CDN import is a defensible default; flag the substitution if you do.

---

## Caveats

- **Fonts come from the Google Fonts CDN.** Inter, JetBrains Mono, Noto Sans SC, and Noto Color Emoji all load from `fonts.gstatic.com` via a single `@import` in [`colors_and_type.css`](./colors_and_type.css)  same URL as production `social-frontend/index.html`. Local WOFF2 snapshots of three of the four families live in [`fonts/`](./fonts/) for offline reference but aren't referenced by the active CSS.
- **No icon library is bundled.** Riposte's chrome uses hand-rolled SVGs and Unicode glyphs. If you build out new screens, follow the conventions above or note your additions.
- **No standalone Riposte logo exists.** The product is wordmark-only; the bat in `assets/` is the maintainer's org identity. If a graphical mark gets designed later, drop it in `assets/` and update this file.
- **Admin panel is out of scope.** It's a separate React SPA under the same repo (`admin-frontend/`) but does not share the social tokens.

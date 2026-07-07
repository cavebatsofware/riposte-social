# Storefront branding contract

riposte-social is a generic platform; it ships no operator branding of its own
beyond a neutral default. When the **business** feature is enabled and a
storefront is configured (the `shop_url` setting), the platform sources brand
imagery **from the storefront**, served at the store's root URL. This lets an
operator brand both their storefront and the social app from a single place
(their storefront project) without modifying or forking the platform.

This document defines the interface a storefront must satisfy. Anyone building
their own storefront (the platform does not ship one; operators provide their
own) should follow it.

## What the platform consumes today

The site/brand **name** is not an asset. It comes from the `SITE_NAME`
environment variable / `site_name` setting and drives the social app's header
wordmark and the browser tab title. Set it per deployment.

The social app sets its favicon at runtime from the configured store:

- `${shop_url}/favicon.svg`  > the tab/bookmark icon
- `${shop_url}/apple-touch-icon.png` > iOS home-screen icon

When no storefront is configured, the platform's own neutral default favicon is
used. The store may be on a different subdomain than the social app (e.g.
`shop.example.com` vs `www.example.com`); the assets are referenced by absolute
store URL, which browsers load cross-origin without issue.

Assets are served by the storefront itself: `bun tooling/cli.ts shop-build <site>` copies the
storefront's static export into `shop-assets/`, which the shop server serves at
its root. So `${shop_url}/favicon.svg` resolves to `shop-assets/favicon.svg`.
The files live in the operator's storefront project, never in riposte-social.

## Required assets (served at the store root)

These names are **fixed**: the platform builds `${shop_url}/favicon.svg` and
`${shop_url}/apple-touch-icon.png` literally, so every storefront must expose
exactly these generic, storefront-agnostic filenames (no brand- or
storefront-specific prefixes).

| Path | Format | Spec |
|---|---|---|
| `/favicon.svg` | SVG | Square, self-contained (no external refs), `viewBox` set. **Brand colors baked in** favicons render in browser chrome and bookmarks outside the app and cannot read the live theme, so do not use `currentColor` here. Must stay legible on both light and dark browser chrome. Design on a 24–32px grid. |
| `/apple-touch-icon.png` | PNG | 180×180. **Opaque background** iOS ignores transparency and composites on black. |

## Recommended assets (raster fallbacks)

| Path | Format | Spec |
|---|---|---|
| `/favicon-32.png` | PNG | 32×32, exact. |
| `/favicon-16.png` | PNG | 16×16, exact. |

Reference these from the storefront's own `<head>` for browsers that prefer
PNG, e.g.:

```html
<link rel="icon" type="image/svg+xml" href="/favicon.svg">
<link rel="icon" type="image/png" sizes="32x32" href="/favicon-32.png">
<link rel="icon" type="image/png" sizes="16x16" href="/favicon-16.png">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
```

## Optional: in-app logo mark

If you want a logo glyph in the social app header (beyond the text wordmark),
expose a mark and surface its URL via the `brand_logo_url` site setting. Unlike
the favicons, this path is **not** fixed: `brand_logo_url` is a full URL you
choose, so the filename is yours (`brand-mark.svg` is just a suggested generic
name).

| Path (suggested) | Format | Spec |
|---|---|---|
| `/brand-mark.svg` | SVG | Uses `fill="currentColor"` / `stroke="currentColor"` so it **recolors per the active theme** (the opposite of the favicon, which has colors baked in). Square `viewBox`, round joins to match the platform's stroke idiom. |

## Rules summary

- **favicon.svg**: colors baked in (theme-independent surface).
- **brand-mark.svg**: `currentColor` (theme-following, in-app surface).
- **PNGs**: exact pixel dimensions as named; `apple-touch-icon.png` opaque.
- All paths are served at the **store root**, so they are reachable at
  `${shop_url}/<path>`.
- Keep the asset set small and self-contained; no external font/image refs.

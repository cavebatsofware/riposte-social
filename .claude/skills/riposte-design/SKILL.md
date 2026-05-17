---
name: riposte-design
description: Use this skill to generate well-branded interfaces and assets for Riposte Social (self-hosted, invite-only social network for family and close friends), either for production or throwaway prototypes / mocks / etc. Contains essential design guidelines, colors, type, fonts, assets, and UI kit components for prototyping.
user-invocable: true
---

Read the [`README.md`](./README.md) file within this skill, and explore the other available files.

If creating visual artifacts (slides, mocks, throwaway prototypes, etc), copy assets out and create static HTML files for the user to view. Link [`colors_and_type.css`](./colors_and_type.css) for tokens and set `<html data-theme="forest">` (or another of the 16 theme ids  see README) to apply a colorway.

If working on production code, you can copy assets and read the rules here to become an expert in designing with this brand. The single source of truth is the upstream repo at <https://github.com/cavebatsofware/riposte-social>; the canonical CSS lives at `social-frontend/src/index.css` (also imported here under [`_source/`](./_source/) for offline reference).

If the user invokes this skill without any other guidance, ask them what they want to build or design, ask some clarifying questions about audience / surface / fidelity, and then act as an expert designer who outputs HTML artifacts *or* production code, depending on the need.

## Quick reference

- **Brand voice:** direct, plain, slightly editorial. Sentence case. "You", never "I/we". Functional emoji only (the six reaction glyphs); no decorative emoji in headings or marketing copy.
- **Default theme:** `forest` (Forest & Cream  deep green primary on warm sand). 7 other colorways available, each with a hand-tuned dark companion.
- **Type:** Inter (body / UI), JetBrains Mono (code / handles), Noto Sans SC (CJK fallback), Noto Color Emoji (reactions). Loaded from Google Fonts CDN.
- **Iconography:** wordmark-only logo. No icon library  hand-rolled inline SVGs (Lucide-adjacent), Unicode glyphs (▾ ▶ × ✓ → ←), and the six reaction emoji are the entire vocabulary.
- **UI kit:** [`ui_kits/riposte/`](./ui_kits/riposte/)  click-thru recreation with Header, BrowseRail, PostCard, ReactionBar, Lightbox, Composer, Login, ThemePicker.

## Don't

- Don't add the cavebatsofware bat logo (`assets/cavebatsofware-mark.png`) to Riposte UI  it's the maintainer's org identity, not the product.
- Don't invent new colorways. The eight named colorways are the system; layer custom oklch shades on top only if you can't get the effect from the existing tokens.
- Don't add bluish-purple gradients, rounded-corner cards with colored left-border accents, or decorative emoji. The system avoids all three.

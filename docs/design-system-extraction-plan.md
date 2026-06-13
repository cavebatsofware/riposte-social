# Extracting `@cavebatsofware/riposte-design-system`

Status: proposal for review. No code written yet.

## Goal

Collapse the duplicated Riposte design layer into one shared, versioned package
that:

- owns the canonical design tokens (the 8-theme palette + base scale) and the
  theme engine;
- ships the shared, app-agnostic React components;
- is consumed by `social-frontend`, by `picnic-table-configurator`, by other
  per-deployment configurators, and (later) by `admin-frontend`;
- lets a downstream SPA supply its own colorway catalog and component
  composition for a different use case.

## Decisions locked

- Name: `@cavebatsofware/riposte-design-system` (npm scope as `riposte-pickers`).
- Layering: **design-system owns the tokens and the theme engine.**
  `riposte-pickers` is refactored to depend on design-system, reversing today's
  direction. There is no circular edge: the generic chassis (`PopoverPicker`,
  `useRovingFocus`) moves down into design-system, and pickers consume it.

```
                  riposte-design-system
        (palette.css [8 themes], base tokens, ThemeProvider/useTheme,
         PopoverPicker, useRovingFocus, shared components, picker i18n)
                      ^                         ^
        depends on    |                         |  depends on
                      |                         |
              riposte-pickers            social-frontend / admin-frontend
        (ThemePicker, LanguagePicker;   picnic-config and other configurators
         consumes tokens + chassis)     (consume design-system + pickers)
```

## Current state (what we are collapsing)

The design layer is already forked into two drifted copies:

| Where | Tokens | Theme engine | Pickers | Shared chassis | Consumed by |
| --- | --- | --- | --- | --- | --- |
| `social-frontend/src/index.css` + `contexts/ThemeContext.tsx` | 8 themes (5 aesthetic + 3 WCAG), lines 1-671 of `index.css` | own untyped copy, richer behavior | own copies (`ThemePicker`, `LanguagePicker`, `PopoverPicker`) | own copy | the social SPA |
| `@cavebatsofware/riposte-pickers` v0.1.0 (tsup, ESM+CJS) | 5 themes only (stale subset, missing the 3 WCAG) | own typed copy | the published pickers | `PopoverPicker`, `useRovingFocus` | picnic-config (`^0.1.0` from npm) |
| `picnic-table-configurator` (Next 15, static export) | none (consumes pickers' palette) | uses pickers | uses pickers | uses pickers | a deployment |

Consequences this migration fixes:

- `social-frontend` does not depend on `riposte-pickers` at all; it maintains a
  parallel hand-edited copy of the same theme engine and pickers.
- The pickers palette is missing `daltonia`, `tritan`, `achroma`, so
  picnic-config today silently offers only 5 of the 8 themes (no accessibility
  themes).

## The one real code merge: the theme engine

`ThemeProvider` / `useTheme` exists in two forms that must become one in
design-system:

- `social-frontend/src/contexts/ThemeContext.tsx`: 8-colorway `COLORWAYS` with
  swatch + accessibility-aware labels, OS `prefers-color-scheme` subscription
  for first-time visitors, and a `setMode` axis (flip light/dark while keeping
  colorway). Untyped JS, hardcoded `STORAGE_KEY = "rs_theme_v1"`, fixed catalog.
- `riposte-pickers/src/theme/ThemeContext.tsx`: fully typed, configurable
  (`DEFAULT_STORAGE_KEY`, injectable colorways), but only 5 colorways and no OS
  tracking.

Design-system ships the **union**:

- typed (the pickers types: `Colorway`, `ThemeMode`, `ThemeContextValue`,
  `ThemeProviderProps`);
- the 8-colorway default catalog from social, with swatch + label metadata;
- OS-preference subscription and `setMode` from social;
- configurable storage key and **injectable colorways** (a downstream
  configurator passes its own catalog via `ThemeProvider` props), with the
  Riposte 8 as the default;
- keep the `THEMES` backward-compat alias only if any consumer still imports it
  (grep says no; drop it).

This is the only file where logic is reconciled rather than moved verbatim.

## Target package layout

Modeled on `riposte-pickers` (tsup, dual ESM/CJS, subpath exports, GPL-3.0-only
headers, `sideEffects: ["*.css"]`).

```
riposte-design-system/            (sibling repo, like riposte-pickers)
  package.json                    name @cavebatsofware/riposte-design-system, v0.1.0
  tsup.config.ts                  entries: index, theme, shared, components, i18n/index
  tsconfig.json
  styles/
    index.css                     @import palette + base + components
    palette.css                   16 [data-theme] blocks (8 themes x light/dark)
    tokens.css                    the :root base scale (font/size/spacing/radius/shadow)
    components.css                shared-component styles lifted from index.css
  src/
    index.ts                      barrel
    theme/
      index.ts
      ThemeContext.tsx            merged engine (see above)
    shared/
      index.ts
      PopoverPicker.tsx           moved down from pickers
      useRovingFocus.ts           moved down from pickers
    components/
      index.ts
      VisibilityBadge.tsx
      SkeletonCard.tsx
      LoadingBar.tsx
      CookieBanner.tsx
      Drawer.tsx                  headless primitive (focus trap, overlay, a11y)
      useFocusTrap.ts             moved from social-frontend/utils
    i18n/
      index.ts                    themeResources (+ optionally base UI strings)
      theme/{en,es,fr,zh,de}.ts
```

Subpath exports: `.`, `./theme`, `./shared`, `./components`, `./i18n`,
`./styles`, `./styles/palette.css`, `./styles/tokens.css`,
`./styles/components.css`.

## Components to extract (and what stays)

App-agnostic, move to design-system `./components`:

- `VisibilityBadge`, `VisibilityMenu`, `VisibilityPicker` (generic visibility
  primitives over the popover chassis; the visibility vocabulary is Riposte
  domain but reusable across Riposte SPAs);
- `SkeletonCard` (+ `SkeletonCard.css`);
- `LoadingBar` (pairs with the `loadingState` util, which also moves);
- `CookieBanner`;
- a **headless `Drawer` primitive** extracted from `MobileDrawer` (focus trap,
  overlay, open/close, escape + a11y wiring); social-frontend keeps the
  app-specific drawer contents (nav links, pickers) and composes them into the
  primitive;
- the shared chassis already named: `PopoverPicker`, `useRovingFocus`.

Stays in `social-frontend` (app shell / feature-coupled):

- `Layout` (+ `Layout.css`), `BrowseRail`, `UserMenu`, `ComposeMenu`: these wire
  in routing, auth context, site config, and the feed rail. Keep them in the app
  for now; revisit `Layout` as a slotted shell in a later pass.
- everything under `features/*`.

Boundary calls:

- `MobileDrawer`: DECIDED, extract the headless `Drawer` primitive now (focus
  trap, overlay, open/close, escape/a11y). The app-specific contents (nav links,
  pickers) stay in social-frontend and compose into the primitive. The existing
  `utils/useFocusTrap.ts` moves into the primitive.
- `VisibilityBadge` / `VisibilityMenu` / `VisibilityPicker`: DECIDED, deferred.
  Reading the code showed they are domain-coupled, not generic primitives:
  `VisibilityMenu` calls `patchPostVisibility` (the posts API), and all three
  encode Riposte's `private/public/commenters/posters` tiers on the
  `feed`/`compose` i18n namespaces. They stay in social-frontend. They are
  shared Riposte *domain* UI (admin will want them), which is a different
  concern from the generic design system a picnic-style configurator consumes;
  revisit as part of admin-frontend normalization, possibly as a separate
  `riposte-domain` component package.

Future direction (noted, not in scope): the pickers are "basically components",
so `riposte-pickers` may eventually fold into `riposte-design-system` as a
`./pickers` subpath. The current layering already makes this cheap: pickers
depends on design-system (not the reverse), so merging is a move, not a
dependency rewrite.

## i18n boundary (hybrid, decided)

- design-system ships the **picker** strings (as pickers does today under
  `src/i18n/theme`) **plus a small base-UI bundle** for the shared components,
  exported under `./i18n` as resources a consumer merges into their own i18next
  instance (same pattern picnic-config uses for `themeResources`). All 5
  languages, batteries-included.
- Each shared component **also accepts optional label props** that override the
  bundle. Precedence: explicit prop > merged bundle string > hardcoded English
  default. So a consumer that merges nothing and passes nothing still renders
  (English); a consumer with its own vocabulary injects via props; the common
  case merges the bundle and gets all 5 languages for free.
- The app content namespaces (`feed`, `compose`, `settings`, `auth`, `browse`,
  `articles`) and the HTTP-backend loading setup stay in `social-frontend`. The
  5-language base (`en, es, fr, zh, de`) and `i18n.ts` init remain app-owned;
  design-system does not impose an i18next init on consumers, it only exports
  resource bundles a consumer merges into their own instance (the pattern
  picnic-config already uses via `themeResources`).

## Sequenced migration

Each phase is independently reviewable and leaves every app building.

**Phase 0: scaffold.** Create the `riposte-design-system` repo/package: tsup
config, tsconfig, package.json, GPL headers, empty barrels. No behavior yet.

**Phase 1: tokens + theme engine + chassis.** Move into design-system:
`palette.css` (the 8-theme slab, `index.css` lines 1-580 split into
`palette.css`; the `:root` base block lines 581-671 into `tokens.css`), the
merged `ThemeContext`, and `PopoverPicker` + `useRovingFocus`. Publish `0.1.0`
(or local-link for dev, see below).

**Phase 2: refactor riposte-pickers to depend on design-system (v0.2.0,
breaking).** Delete `styles/palette.css`, `src/theme/ThemeContext.tsx`,
`src/shared/PopoverPicker.tsx`, `src/shared/useRovingFocus.ts` from pickers.
Re-point `ThemePicker`/`LanguagePicker` imports to
`@cavebatsofware/riposte-design-system/theme` and `.../shared`. Add
design-system as a dependency (or peer). Keep shipping `picker.css` /
`language.css` (component styles that consume tokens). Pickers no longer ships
tokens.

**Phase 3: extract shared components.** Move `VisibilityBadge/Menu/Picker`,
`SkeletonCard`, `LoadingBar`, `CookieBanner` (+ their CSS into
`components.css`, + the `loadingState` util) into design-system `./components`.

**Phase 4: rewire social-frontend.** Add `@cavebatsofware/riposte-design-system`
and `@cavebatsofware/riposte-pickers` as deps. Replace
`contexts/ThemeContext.tsx` import sites with design-system; delete the local
copy. Replace `components/ThemePicker.tsx`, `LanguagePicker.tsx`,
`PopoverPicker.tsx`, and the extracted shared components with imports. Split
`index.css`: delete lines 1-671 (now in design-system) and the lifted
shared-component sections; import design-system styles in `main.tsx`
(`import "@cavebatsofware/riposte-design-system/styles"`); keep only
app/feature-specific CSS in the app. Run `bun run lint`, `bun run check:i18n`,
`bun run build:social`, and the a11y smoke (`bun run a11y:smoke`) to confirm
parity (all 8 themes still switch, pickers still work).

**Phase 5: upgrade picnic-table-configurator.** Bump
`@cavebatsofware/riposte-pickers` to `^0.2.0`, add
`@cavebatsofware/riposte-design-system`, swap
`import "@cavebatsofware/riposte-pickers/styles"` for the design-system styles
import. Net effect: picnic gains the 3 WCAG themes for free. Verify the static
export still builds.

**Phase 6 (later, out of this scope): admin-frontend normalization.** Adopt
design-system tokens + chassis incrementally; not part of the first cut, but the
package boundary is designed so admin can opt in component-by-component.

## Cross-cutting concerns to settle in review

- **Local dev linking.** DECIDED. `social-frontend` (and the pickers refactor)
  consume design-system via `file:../riposte-design-system` for instant local
  iteration; `picnic-table-configurator` pins real published versions. CI for
  riposte-social must resolve the `file:` path (the sibling checkout) or fall
  back to a published version; settle the CI checkout layout in Phase 4.
- **Bun bundling of package CSS.** `social-frontend` is bundled by Bun from
  `index.html`. Importing CSS from a node_modules package works the same way
  picnic-config imports it under Next; verify Bun inlines
  `@cavebatsofware/riposte-design-system/styles` and its `@import` chain into the
  output (it should; confirm in Phase 4 build).
- **Versioning.** design-system `0.1.0`; pickers `0.1.0 -> 0.2.0` (breaking, it
  drops tokens). picnic-config pins the new majors.
- **License headers.** Every moved file keeps its GPL-3.0-only header (matches
  pickers and the repo).
- **No drift reintroduced.** After Phase 4, `social-frontend` must have zero
  local `[data-theme]` blocks and zero local copy of the theme engine; a CI grep
  guard can enforce it.
- **Theme parity gate.** Before/after Phase 4, the a11y smoke and a manual
  8-theme x light/dark sweep confirm no visual or token regressions.

## Open questions for you

1. i18n: DECIDED, hybrid. Design-system ships picker + base-UI bundles (all 5
   languages) under `./i18n`; shared components take optional label props that
   override the bundle (prop > bundle > English default).
2. `MobileDrawer`: DECIDED, extract headless `Drawer` primitive now; app keeps
   the drawer contents.
3. Dev linking: DECIDED, `file:` for social dev, publish for picnic.
4. Scope: DECIDED, first cut stops at Phase 5 (social + picnic). admin-frontend
   (Phase 6) is a separate later effort.

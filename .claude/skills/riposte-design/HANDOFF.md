# How to use this bundle

You've downloaded the **Riposte Design System** as a Claude Code skill.

## Install as a reusable skill

Unzip into one of:

| Location | Scope |
|---|---|
| `~/.claude/skills/riposte-design/` | User-level — available in every Claude Code session, in any repo. |
| `<your-riposte-repo>/.claude/skills/riposte-design/` | Repo-level — checked into the riposte-social repo so the team gets it automatically. |

Reload the Claude Code VS Code extension. The skill is auto-discovered via [`SKILL.md`](./SKILL.md) at the bundle root.

Then invoke it from any session:

> Use the **riposte-design** skill to build a settings page mock.

> Apply the **riposte-design** tokens to this new component.

## Or use it as plain reference

If you don't want it as a skill, just unzip it anywhere and point Claude Code at the folder. The [`README.md`](./README.md) is fully self-contained and Claude Code will read it on its own.

## What's inside

| Path | What it is |
|---|---|
| [`README.md`](./README.md) | The system itself — visual foundations, content fundamentals, iconography, manifest. **Start here.** |
| [`SKILL.md`](./SKILL.md) | Claude Code skill front-matter so the bundle is auto-discoverable. |
| [`colors_and_type.css`](./colors_and_type.css) | Drop-in stylesheet with every color, type, spacing, radius, and shadow token. |
| [`fonts/`](./fonts/) | WOFF2 snapshots of the four CDN-loaded font families, with a manifest. |
| [`assets/`](./assets/) | Brand mark for the maintainer's org (cavebatsofware). The Riposte product itself is wordmark-only. |
| [`preview/`](./preview/) | 25 standalone HTML cards rendering every token, badge, button, and core component in isolation. |
| [`ui_kits/riposte/`](./ui_kits/riposte/) | Full interactive UI-kit recreation of the Riposte social-frontend chrome — header, feed, post cards, reactions, lightbox, compose, login, theme picker. |
| [`_source/`](./_source/) | Imported snapshot of [`cavebatsofware/riposte-social`](https://github.com/cavebatsofware/riposte-social) `social-frontend/` for offline reference. The live repo is the source of truth. |

# Riposte UI Kit

A click-thru recreation of the Riposte Social product surface  header, browse rail, feed of post cards, reaction picker, theme switcher, media lightbox, and login.

## Files

| File | What it is |
|---|---|
| `index.html` | Mount point. Loads React + Babel, the design tokens, and every component. Renders the demo Feed. |
| `app.jsx` | Demo app: state + router + sample data. |
| `Header.jsx` | Sticky header  wordmark, primary nav, language/theme pickers, user menu / sign-in. |
| `BrowseRail.jsx` | Left rail with Categories / Albums / People accordion groups. |
| `PostCard.jsx` | Feed post card  author meta, visibility badge, body, media, actions, top comments. |
| `ReactionBar.jsx` | Facebook-6 emoji bar with hover/long-press picker and live count badges. |
| `Composer.jsx` | Minimal `/compose` form  body markdown + visibility chooser + publish CTA. |
| `Login.jsx` | Sign-in form with the invite-only notice card. |
| `InviteSplash.jsx` | Modal welcome dialog for invited users on the public feed. |
| `Lightbox.jsx` | Full-bleed media viewer with the peeking engagement panel (~24px peek + scroll). |
| `ThemePicker.jsx` | Colorway grid + light/dark radio row. |

## Tokens

The kit consumes [`../../colors_and_type.css`](../../colors_and_type.css) directly  no token redefinitions. Theme is applied by setting `<html data-theme="forest">` (or any of the other 16 theme ids).

## Coverage caveats

- This is a **visual + click-thru** recreation. There is no real API; reactions, posts, and follows live in component state.
- The Compose page omits the markdown live preview and the drag-and-drop attachment surface that the real Riposte ships.
- The Post permalink page renders inline inside the same SPA shell; the real app uses react-router.
- The invite-accept and OIDC flows are not represented.
- Settings, Albums, Profile, People, and Categories management pages are out of scope here  see [`_source/pages/`](../../_source/pages/) for the originals.

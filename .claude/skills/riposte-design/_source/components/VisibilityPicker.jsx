import { useTranslation } from "react-i18next";

/// Shared visibility-tier picker used by Compose (post) and ComposeAlbum.
/// Renders the same `<fieldset className="compose-field">` + 4 pill buttons
/// in both places so the four tiers (`private`, `public`, `commenters`,
/// `posters`) are presented consistently and the tier set evolves in one
/// file.
///
/// Props:
/// - `value`: currently-selected visibility id (one of `VISIBILITIES`)
/// - `onChange(newId)`: called with the new id when a pill is clicked
/// - `legend` (optional): override the catalog's default "Visibility"
///   legend with a literal string — used by the rare consumer that needs
///   different framing. Default reads from `compose:visibility.legend`.

export const VISIBILITIES = ["private", "public", "commenters", "posters"];

export default function VisibilityPicker({ value, onChange, legend }) {
  // Pull `compose` for the legend wording and `feed` for each tier's
  // name + description (the same labels that render on PostCard badges
  // and in the VisibilityMenu, so a single edit ripples everywhere).
  const { t: tCompose } = useTranslation("compose");
  const { t: tFeed } = useTranslation("feed");
  return (
    <fieldset className="compose-field">
      <legend>{legend ?? tCompose("visibility.legend")}</legend>
      <div className="visibility-options">
        {VISIBILITIES.map((id) => (
          <button
            key={id}
            type="button"
            className={`visibility-pill ${value === id ? "selected" : ""}`}
            onClick={() => onChange(id)}
          >
            <span className="visibility-pill-label">
              {tFeed(`visibility.${id}.name`)}
            </span>
            <span className="visibility-pill-desc">
              {tFeed(`visibility.${id}.desc`)}
            </span>
          </button>
        ))}
      </div>
    </fieldset>
  );
}

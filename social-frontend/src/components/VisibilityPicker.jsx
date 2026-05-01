/// Shared visibility-tier picker used by Compose (post) and ComposeAlbum.
/// Renders the same `<fieldset className="compose-field">` + 4 pill buttons
/// in both places so the four tiers (`private`, `public`, `commenters`,
/// `posters`) are presented consistently and the tier set evolves in one
/// file.
///
/// Props:
/// - `value`: currently-selected visibility id (one of `VISIBILITIES[].id`)
/// - `onChange(newId)`: called with the new id when a pill is clicked
/// - `legend` (optional): override the "Visibility" legend text — keep the
///   default in compose flows.

export const VISIBILITIES = [
  { id: "private", label: "Private", desc: "Only you can see this" },
  { id: "public", label: "Public", desc: "Anyone with the link" },
  { id: "commenters", label: "Commenters", desc: "Friends with invites" },
  { id: "posters", label: "Posters", desc: "Family authors only" },
];

export default function VisibilityPicker({ value, onChange, legend = "Visibility" }) {
  return (
    <fieldset className="compose-field">
      <legend>{legend}</legend>
      <div className="visibility-options">
        {VISIBILITIES.map((v) => (
          <button
            key={v.id}
            type="button"
            className={`visibility-pill ${value === v.id ? "selected" : ""}`}
            onClick={() => onChange(v.id)}
          >
            <span className="visibility-pill-label">{v.label}</span>
            <span className="visibility-pill-desc">{v.desc}</span>
          </button>
        ))}
      </div>
    </fieldset>
  );
}

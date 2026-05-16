/* global React */
const { useState: useStateCmp } = React;

/// Minimal Compose form — markdown body + visibility chooser + publish.
function Composer({ onPublish, onCancel }) {
  const [body, setBody] = useStateCmp("");
  const [visibility, setVisibility] = useStateCmp("commenters");
  const [category, setCategory] = useStateCmp("");

  return (
    <div className="compose-card">
      <h1 style={{ margin: 0, fontSize: "var(--font-size-2xl)", color: "var(--color-primary)" }}>New post</h1>
      <div>
        <label htmlFor="compose-body" style={{ display: "block", fontSize: "var(--font-size-sm)", fontWeight: 600, marginBottom: "var(--spacing-2)" }}>Body (markdown)</label>
        <textarea
          id="compose-body"
          className="compose-textarea"
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder={"What's on your mind?\n\n**Bold**, *italic*, [links](https://example.com), and ![images](url) all work."}
        />
        <p style={{ marginTop: "var(--spacing-2)", fontSize: "var(--font-size-sm)", color: "var(--color-muted)" }}>
          Markdown is rendered server-side. Inline images and links are first-class. HTML is sanitized.
        </p>
      </div>

      <div className="compose-row">
        <label htmlFor="compose-visibility">Visibility</label>
        <select id="compose-visibility" value={visibility} onChange={(e) => setVisibility(e.target.value)}>
          <option value="public">Public</option>
          <option value="commenters">Commenters</option>
          <option value="posters">Posters</option>
          <option value="private">Private</option>
        </select>
        <label htmlFor="compose-category" style={{ marginLeft: "var(--spacing-4)" }}>Category</label>
        <select id="compose-category" value={category} onChange={(e) => setCategory(e.target.value)}>
          <option value="">Uncategorized</option>
          <option value="family">Family</option>
          <option value="garden">Garden</option>
        </select>
      </div>

      <div style={{ borderTop: "1px solid var(--color-divider)", paddingTop: "var(--spacing-3)", display: "flex", alignItems: "center", justifyContent: "space-between", fontSize: "var(--font-size-sm)", color: "var(--color-muted)" }}>
        <span>Saving will publish immediately.</span>
        <div style={{ display: "flex", gap: "var(--spacing-2)" }}>
          <button className="btn-secondary" onClick={onCancel}>Cancel</button>
          <button className="btn-primary" onClick={() => onPublish && onPublish({ body, visibility, category })} disabled={!body.trim()}>Publish</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { Composer });

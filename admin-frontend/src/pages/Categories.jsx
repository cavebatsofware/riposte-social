import { useState, useEffect } from "react";
import Layout from "../components/Layout";
import { fetchApi } from "../utils/api";
import "./Categories.css";

/// Admin CRUD for categories (Phase 9e). Flat list, ordered by ordinal.
/// Categories live under `/api/admin/categories` for write paths and the
/// public `/api/categories` for the list (used here too — admins see the
/// same data the social-frontend rail does, but with edit affordances).
function Categories() {
  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");

  // Edit-in-place: keep a local copy of each row's editable fields
  // alongside the server-truth row. Save dispatches a PATCH only when
  // any field has changed.
  const [drafts, setDrafts] = useState({});

  // New-row form state.
  const [newName, setNewName] = useState("");
  const [newSlug, setNewSlug] = useState("");
  const [newColor, setNewColor] = useState("");

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    setLoading(true);
    try {
      const response = await fetchApi("/api/categories");
      if (!response.ok) throw new Error("Failed to load categories");
      const data = await response.json();
      setCategories(data.categories || []);
      // Hydrate drafts from server values.
      const next = {};
      for (const c of data.categories || []) {
        next[c.id] = {
          name: c.name,
          slug: c.slug,
          ordinal: c.ordinal,
          color: c.color || "",
        };
      }
      setDrafts(next);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  function setDraft(id, field, value) {
    setDrafts((prev) => ({
      ...prev,
      [id]: { ...prev[id], [field]: value },
    }));
  }

  async function handleSave(id) {
    setError("");
    setSuccess("");
    const draft = drafts[id];
    if (!draft) return;
    const body = {
      name: draft.name,
      slug: draft.slug,
      ordinal: Number(draft.ordinal) || 0,
      color: draft.color,
    };
    try {
      const response = await fetchApi(`/api/admin/categories/${id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to save category");
      }
      setSuccess("Saved.");
      await refresh();
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleDelete(id) {
    if (!window.confirm("Delete this category? Posts and albums in it will become uncategorized.")) {
      return;
    }
    setError("");
    setSuccess("");
    try {
      const response = await fetchApi(`/api/admin/categories/${id}`, {
        method: "DELETE",
      });
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to delete category");
      }
      setSuccess("Deleted.");
      await refresh();
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleCreate(e) {
    e.preventDefault();
    setError("");
    setSuccess("");
    if (!newName.trim()) {
      setError("Name is required.");
      return;
    }
    const body = { name: newName.trim() };
    if (newSlug.trim()) body.slug = newSlug.trim();
    if (newColor.trim()) body.color = newColor.trim();
    try {
      const response = await fetchApi("/api/admin/categories", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to create category");
      }
      setNewName("");
      setNewSlug("");
      setNewColor("");
      setSuccess("Created.");
      await refresh();
    } catch (err) {
      setError(err.message);
    }
  }

  return (
    <Layout>
      <div className="categories-page">
        <header className="page-header">
          <div>
            <h1>Categories</h1>
            <p className="page-header-subtitle">
              Categories group posts and albums in the social-frontend's left
              rail. Deleting a category leaves anything in it uncategorized.
            </p>
          </div>
        </header>

        {error && <div className="alert alert-error">{error}</div>}
        {success && <div className="alert alert-success">{success}</div>}

        <section className="card categories-create">
          <h2 className="card-title">New category</h2>
          <form onSubmit={handleCreate}>
            <div className="form-group">
              <label htmlFor="new-cat-name">Name</label>
              <input
                id="new-cat-name"
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                required
                maxLength={80}
                placeholder="Family"
              />
            </div>
            <div className="form-group">
              <label htmlFor="new-cat-slug">Slug</label>
              <input
                id="new-cat-slug"
                type="text"
                value={newSlug}
                onChange={(e) => setNewSlug(e.target.value)}
                placeholder="auto-derived from name"
                maxLength={50}
              />
              <small>
                Lowercase letters, digits, and hyphens. Used in URLs like
                <code> /?category=family</code>.
              </small>
            </div>
            <div className="form-group">
              <label htmlFor="new-cat-color">Color</label>
              <div
                className={`categories-color-input ${newColor ? "" : "is-empty"}`}
              >
                <input
                  id="new-cat-color-picker"
                  type="color"
                  value={newColor || "#000000"}
                  onChange={(e) => setNewColor(e.target.value)}
                  className="categories-color-picker"
                  aria-label="Pick a color"
                  title={newColor ? newColor : "No color set"}
                />
                <input
                  id="new-cat-color"
                  type="text"
                  className="categories-color-text"
                  value={newColor}
                  onChange={(e) => setNewColor(e.target.value)}
                  placeholder="#aabbcc (leave empty for no tint)"
                  maxLength={9}
                />
                {newColor && (
                  <button
                    type="button"
                    className="categories-color-clear"
                    onClick={() => setNewColor("")}
                    aria-label="Clear color"
                    title="Clear color"
                  >
                    ×
                  </button>
                )}
              </div>
              <small>Optional — used to tint the category chip on posts.</small>
            </div>
            <div className="form-actions">
              <button type="submit" className="btn-primary">
                Create category
              </button>
            </div>
          </form>
        </section>

        <section className="card categories-list-section">
          <h2 className="card-title">
            All categories ({categories.length})
          </h2>
          {loading ? (
            <p>Loading…</p>
          ) : categories.length === 0 ? (
            <p className="muted">No categories yet.</p>
          ) : (
            <div className="table-wrapper">
              <table className="categories-table">
                <thead>
                  <tr>
                    <th className="categories-col-ordinal">Order</th>
                    <th>Name</th>
                    <th>Slug</th>
                    <th>Color</th>
                    <th className="categories-col-actions"></th>
                  </tr>
                </thead>
                <tbody>
                {categories.map((c) => {
                  const d = drafts[c.id] || {};
                  return (
                    <tr key={c.id}>
                      <td>
                        <input
                          type="number"
                          value={d.ordinal ?? c.ordinal}
                          onChange={(e) => setDraft(c.id, "ordinal", e.target.value)}
                          className="categories-input categories-input-ordinal"
                        />
                      </td>
                      <td>
                        <input
                          type="text"
                          value={d.name ?? c.name}
                          onChange={(e) => setDraft(c.id, "name", e.target.value)}
                          maxLength={80}
                          className="categories-input"
                        />
                      </td>
                      <td>
                        <input
                          type="text"
                          value={d.slug ?? c.slug}
                          onChange={(e) => setDraft(c.id, "slug", e.target.value)}
                          maxLength={50}
                          className="categories-input"
                        />
                      </td>
                      <td>
                        <div className="categories-color-cell">
                          {(d.color || c.color) && (
                            <span
                              className="categories-color-swatch"
                              style={{ backgroundColor: d.color || c.color }}
                              aria-hidden="true"
                            />
                          )}
                          <input
                            type="text"
                            value={d.color ?? c.color ?? ""}
                            onChange={(e) => setDraft(c.id, "color", e.target.value)}
                            placeholder="#aabbcc"
                            maxLength={9}
                            className="categories-input categories-input-color"
                          />
                        </div>
                      </td>
                      <td className="categories-row-actions">
                        <button
                          type="button"
                          className="btn-secondary"
                          onClick={() => handleSave(c.id)}
                        >
                          Save
                        </button>
                        <button
                          type="button"
                          className="btn-danger"
                          onClick={() => handleDelete(c.id)}
                        >
                          Delete
                        </button>
                      </td>
                    </tr>
                  );
                })}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </div>
    </Layout>
  );
}

export default Categories;

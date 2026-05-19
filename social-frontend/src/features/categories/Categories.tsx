import { useCallback, useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import {
  createCategory,
  deleteCategory,
  fetchCategories,
  fetchCategoryMembers,
  fetchProfilesForPicker,
  replaceCategoryMembers,
  updateCategory,
} from "./api";
import { useAuth } from "../../contexts/AuthContext";
import { useSiteConfig } from "../../contexts/SiteConfigContext";
import { useFocusTrap } from "../../utils/useFocusTrap";
import Layout from "../../components/Layout";
import "./Categories.css";

const VISIBILITY_OPTIONS = [
  "public",
  "commenters",
  "posters",
  "private",
  "user_list",
];

/// True when the row's draft differs from its server-truth values. Used
/// to swap between Save (dirty) and Delete (clean) on the manage row.
function isDirty(row, draft) {
  if (!draft) return false;
  return (
    draft.name !== row.name ||
    draft.slug !== row.slug ||
    Number(draft.ordinal) !== row.ordinal ||
    (draft.color || "") !== (row.color || "") ||
    draft.visibility !== row.visibility
  );
}

/// Combined read + manage page for categories. Replaces both the
/// previous read-only overflow page and the admin-frontend's Categories
/// page. Anyone who can read a category sees it here; rows the caller
/// can manage (admin always; poster with gate-on for rows they created)
/// expose edit/delete/membership controls inline.
export default function Categories() {
  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");
  const { user } = useAuth();
  const { config: siteConfig } = useSiteConfig();

  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");

  // New-row form state.
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [newSlug, setNewSlug] = useState("");
  const [newColor, setNewColor] = useState("");
  const [newVisibility, setNewVisibility] = useState("public");

  // Per-row drafts for inline editing.
  const [drafts, setDrafts] = useState({});

  // Member-edit modal state.
  const [memberModalCat, setMemberModalCat] = useState(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const r = await fetchCategories();
      if (!r.ok) throw new Error(t("categories.loadFailed"));
      const data = await r.json();
      setCategories(data.categories || []);
      const next = {};
      for (const c of data.categories || []) {
        next[c.id] = {
          name: c.name,
          slug: c.slug,
          ordinal: c.ordinal,
          color: c.color || "",
          visibility: c.visibility || "public",
        };
      }
      setDrafts(next);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    // refresh() loads the category list and sets loading flags before the
    // first await; the new compiler-aware hook rule flags that as a
    // cascading render, but the spinner-then-data sequence is intentional.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    refresh();
  }, [refresh]);

  function setDraft(id, field, value) {
    setDrafts((prev) => ({
      ...prev,
      [id]: { ...prev[id], [field]: value },
    }));
  }

  // The caller can create a new category when they're an admin (always),
  // or a poster while the gate is on. The site-config payload only
  // exposes the gate to admin/poster, so commenters get falsy.
  const canCreate = useMemo(() => {
    if (!user) return false;
    if (user.role === "administrator") return true;
    if (
      user.role === "poster" &&
      siteConfig?.poster_category_management_enabled !== false
    ) {
      return true;
    }
    return false;
  }, [user, siteConfig]);

  async function handleCreate(e) {
    e.preventDefault();
    setError("");
    setSuccess("");
    if (!newName.trim()) {
      setError(t("categories.nameRequired"));
      return;
    }
    const body: { name: string; visibility: string; slug?: string; color?: string } = {
      name: newName.trim(),
      visibility: newVisibility,
    };
    if (newSlug.trim()) body.slug = newSlug.trim();
    if (newColor.trim()) body.color = newColor.trim();
    try {
      const r = await createCategory(body);
      if (!r.ok) {
        const data = await r.json().catch(() => ({}));
        throw new Error(data.error || t("categories.createFailed"));
      }
      setNewName("");
      setNewSlug("");
      setNewColor("");
      setNewVisibility("public");
      setCreating(false);
      setSuccess(t("categories.createSuccess"));
      await refresh();
    } catch (err) {
      setError(err.message);
    }
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
      visibility: draft.visibility,
    };
    try {
      const r = await updateCategory(id, body);
      if (!r.ok) {
        const data = await r.json().catch(() => ({}));
        throw new Error(data.error || t("categories.saveFailed"));
      }
      setSuccess(t("categories.saveSuccess"));
      await refresh();
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleDelete(id) {
    if (!window.confirm(t("categories.deleteConfirm"))) return;
    setError("");
    setSuccess("");
    try {
      const r = await deleteCategory(id);
      if (!r.ok && r.status !== 204) {
        const data = await r.json().catch(() => ({}));
        throw new Error(data.error || t("categories.deleteFailed"));
      }
      setSuccess(t("categories.deleteSuccess"));
      await refresh();
    } catch (err) {
      setError(err.message);
    }
  }

  return (
    <Layout>
      <header className="page-header">
        <h1>{t("categories.pageTitle")}</h1>
        {canCreate && (
          <button
            type="button"
            className="btn-primary"
            onClick={() => setCreating((v) => !v)}
          >
            {creating ? tCommon("actions.cancel") : t("categories.newCta")}
          </button>
        )}
      </header>

      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}
      {success && (
        <div className="alert alert-success" role="status">
          {success}
        </div>
      )}

      {creating && canCreate && (
        <section className="categories-card">
          <form
            onSubmit={handleCreate}
            aria-label={t("categories.createCta")}
          >
            <div className="compose-field">
              <label htmlFor="cat-new-name">{t("categories.nameLabel")}<span aria-hidden="true"> *</span></label>
              {/* eslint-disable-next-line a11yinspect/required-element-warning -- rule fires unconditionally; asterisk in label above is the visual indicator */}
              <input
                id="cat-new-name"
                name="name"
                type="text"
                autoComplete="off"
                className="categories-input"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                required
                maxLength={80}
                placeholder={t("categories.namePlaceholder")}
              />
            </div>
            <div className="compose-field">
              <label htmlFor="cat-new-slug">{t("categories.slugLabel")}</label>
              <input
                id="cat-new-slug"
                name="slug"
                type="text"
                className="categories-input"
                value={newSlug}
                onChange={(e) => setNewSlug(e.target.value)}
                placeholder={t("categories.slugPlaceholder")}
                maxLength={50}
              />
            </div>
            <div className="compose-field">
              <label htmlFor="cat-new-vis">{t("categories.visibilityLabel")}</label>
              <select
                id="cat-new-vis"
                name="visibility"
                className="categories-input"
                value={newVisibility}
                onChange={(e) => setNewVisibility(e.target.value)}
              >
                {VISIBILITY_OPTIONS.map((v) => (
                  <option key={v} value={v}>
                    {t(`visibility.${v}.name`, { ns: "feed" })}
                  </option>
                ))}
              </select>
            </div>
            <div className="compose-field">
              <label htmlFor="cat-new-color">{t("categories.colorLabel")}</label>
              <ColorInput value={newColor} onChange={setNewColor} idPrefix="cat-new" />
              <p className="form-hint">{t("categories.colorHint")}</p>
            </div>
            <div className="categories-form-actions">
              <button type="submit" className="btn-primary">
                {t("categories.createCta")}
              </button>
            </div>
          </form>
        </section>
      )}

      {loading && <p className="muted">{tCommon("loading")}</p>}

      {!loading && categories.length === 0 && (
        <p className="muted">{t("categories.empty")}</p>
      )}

      <div className="categories-list">
        {categories.map((c) => {
          const d = drafts[c.id] || {};
          const isUserList =
            (d.visibility || c.visibility) === "user_list";
          if (!c.manageable) {
            // Read-only row.
            return (
              <Link
                key={c.id}
                to={`/?category=${encodeURIComponent(c.slug)}`}
                className="categories-overflow-row"
              >
                {c.color && (
                  <span
                    className="categories-overflow-swatch"
                    style={{ backgroundColor: c.color }}
                    aria-hidden="true"
                  />
                )}
                <span className="categories-overflow-name">{c.name}</span>
                <span className="categories-overflow-vis">
                  {t(`visibility.${c.visibility}.name`, { ns: "feed" })}
                </span>
                <span className="categories-overflow-slug">@{c.slug}</span>
              </Link>
            );
          }
          // Manageable row  inline edit. Controls are grouped into
          // text / pickers / actions so flex-wrap moves them as units
          // when the row narrows. Save and Delete swap based on dirty
          // state: a clean row shows Delete, a row with unsaved changes
          // shows Save (the destructive action stays one click away
          // either way, just not visible simultaneously).
          const dirty = isDirty(c, d);
          return (
            <div className="categories-manage-row" key={c.id}>
              <div className="categories-row-group categories-row-text">
                <input
                  type="text"
                  name={`row-name-${c.id}`}
                  value={d.name ?? c.name}
                  onChange={(e) => setDraft(c.id, "name", e.target.value)}
                  maxLength={80}
                  aria-label={t("categories.nameLabel")}
                  className="categories-input categories-input-name"
                />
                <input
                  type="text"
                  name={`row-slug-${c.id}`}
                  value={d.slug ?? c.slug}
                  onChange={(e) => setDraft(c.id, "slug", e.target.value)}
                  maxLength={50}
                  aria-label={t("categories.slugLabel")}
                  className="categories-input categories-input-slug"
                />
              </div>
              <div className="categories-row-group categories-row-pickers">
                <select
                  name={`row-visibility-${c.id}`}
                  value={d.visibility ?? c.visibility}
                  onChange={(e) => setDraft(c.id, "visibility", e.target.value)}
                  aria-label={t("categories.visibilityLabel")}
                  className="categories-input"
                >
                  {VISIBILITY_OPTIONS.map((v) => (
                    <option key={v} value={v}>
                      {t(`visibility.${v}.name`, { ns: "feed" })}
                    </option>
                  ))}
                </select>
                <ColorInput
                  value={d.color ?? c.color ?? ""}
                  onChange={(v) => setDraft(c.id, "color", v)}
                  idPrefix={`cat-${c.id}`}
                />
              </div>
              <div className="categories-row-group categories-row-actions">
                {isUserList && (
                  <button
                    type="button"
                    className="btn-secondary"
                    onClick={() => setMemberModalCat(c)}
                  >
                    {t("categories.editMembers")}
                  </button>
                )}
                {dirty ? (
                  <button
                    type="button"
                    className="btn-primary"
                    onClick={() => handleSave(c.id)}
                  >
                    {tCommon("actions.save")}
                  </button>
                ) : (
                  <button
                    type="button"
                    className="btn-danger"
                    onClick={() => handleDelete(c.id)}
                  >
                    {tCommon("actions.delete")}
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {memberModalCat && (
        <MemberModal
          category={memberModalCat}
          onClose={() => setMemberModalCat(null)}
        />
      )}
    </Layout>
  );
}

/// Color picker pairing a native `<input type="color">` swatch with a
/// free-form hex text input + a clear button. The native input defaults
/// to black when empty, so the wrapper's `is-empty` class hides the
/// swatch behind a checkered "no color" pattern when no color is set.
function ColorInput({ value, onChange, idPrefix }) {
  const { t } = useTranslation("browse");
  return (
    <div className={`categories-color-input ${value ? "" : "is-empty"}`}>
      <input
        id={`${idPrefix}-color-picker`}
        name={`${idPrefix}-color-picker`}
        type="color"
        value={value || "#000000"}
        onChange={(e) => onChange(e.target.value)}
        className="categories-color-picker"
        aria-label={t("categories.colorLabel")}
        title={value || t("categories.colorEmpty")}
      />
      <input
        id={`${idPrefix}-color`}
        name={`${idPrefix}-color`}
        type="text"
        className="categories-color-text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="#aabbcc"
        maxLength={9}
      />
      {value && (
        <button
          type="button"
          className="categories-color-clear"
          onClick={() => onChange("")}
          aria-label={t("categories.colorClear")}
          title={t("categories.colorClear")}
        >
          ×
        </button>
      )}
    </div>
  );
}

/// Modal for managing a `user_list` category's members. Loads the
/// current members + a list of all profiles, then issues a single
/// `PUT /members` on Save. The dialog mounts a focus trap so Tab
/// stays inside while it's open and Escape routes through onClose;
/// focus returns to the "Edit members" button on dismiss.
function MemberModal({ category, onClose }) {
  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");
  const [members, setMembers] = useState([]);
  const [allProfiles, setAllProfiles] = useState([]);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const trapRef = useFocusTrap<HTMLDivElement>(true, { onEscape: onClose });

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      try {
        const [m, p] = await Promise.all([
          fetchCategoryMembers(category.id),
          fetchProfilesForPicker(),
        ]);
        if (!m.ok) throw new Error(t("categories.membersLoadFailed"));
        if (!p.ok) throw new Error(t("categories.profilesLoadFailed"));
        const md = await m.json();
        const pd = await p.json();
        if (!cancelled) {
          setMembers(md.members || []);
          setAllProfiles(pd.profiles || []);
        }
      } catch (err) {
        if (!cancelled) setError(err.message);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [category.id, t]);

  function toggle(profile) {
    setMembers((prev) => {
      const idx = prev.findIndex((m) => m.user_id === profile.user_id);
      if (idx >= 0) {
        return prev.filter((_, i) => i !== idx);
      }
      return [
        ...prev,
        {
          user_id: profile.user_id,
          handle: profile.handle,
          display_name: profile.display_name,
          created_at: new Date().toISOString(),
        },
      ];
    });
  }

  async function handleSave() {
    setError("");
    setSaving(true);
    try {
      const r = await replaceCategoryMembers(
        category.id,
        members.map((m) => m.user_id),
      );
      if (!r.ok) {
        const data = await r.json().catch(() => ({}));
        throw new Error(data.error || t("categories.membersSaveFailed"));
      }
      onClose();
    } catch (err) {
      setError(err.message);
    } finally {
      setSaving(false);
    }
  }

  const memberIds = useMemo(
    () => new Set(members.map((m) => m.user_id)),
    [members],
  );
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return allProfiles;
    return allProfiles.filter(
      (p) =>
        p.handle.toLowerCase().includes(q) ||
        (p.display_name || "").toLowerCase().includes(q),
    );
  }, [allProfiles, search]);

  return (
    // Backdrop click is a mouse-only convenience; keyboard users dismiss
    // the dialog via Escape (handled by the focus trap) or by tabbing to
    // the explicit close button inside. The inner stopPropagation keeps
    // a click on the dialog body from bubbling up and closing the modal.
    // eslint-disable-next-line a11yinspect/click-handler-warning
    <div className="modal-backdrop" onClick={onClose}>
      {/* eslint-disable-next-line a11yinspect/dialog-element-warning -- focus trap applied via useFocusTrap through trapRef; not statically detectable */}
      <div
        className="modal categories-member-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="member-modal-title"
        aria-busy={loading || saving}
        ref={trapRef}
        // eslint-disable-next-line a11yinspect/click-handler-warning
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id="member-modal-title">
          {t("categories.membersModalTitle", { name: category.name })}
        </h2>

        {error && (
          <div className="alert alert-error" role="alert">
            {error}
          </div>
        )}

        {loading ? (
          <p className="muted">{tCommon("loading")}</p>
        ) : (
          <>
            <label
              htmlFor="member-modal-search"
              className="sr-only"
            >
              {t("categories.membersSearchPlaceholder")}
            </label>
            <input
              id="member-modal-search"
              name="member-search"
              type="search"
              placeholder={t("categories.membersSearchPlaceholder")}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="categories-input"
            />

            <div
              className="categories-member-list"
              role="group"
              aria-labelledby="member-modal-title"
            >
              {filtered.map((p) => {
                const checked = memberIds.has(p.user_id);
                return (
                  <label key={p.user_id} className="categories-member-row">
                    <input
                      type="checkbox"
                      name={`member-${p.user_id}`}
                      aria-label={p.display_name || p.handle}
                      checked={checked}
                      onChange={() => toggle(p)}
                    />
                    <span className="categories-member-name">
                      {p.display_name || p.handle}
                    </span>
                    <span className="categories-member-handle">@{p.handle}</span>
                  </label>
                );
              })}
              {filtered.length === 0 && (
                <p className="muted">{t("categories.membersNoMatches")}</p>
              )}
            </div>
          </>
        )}

        <div className="form-actions">
          <button
            type="button"
            className="btn-secondary"
            onClick={onClose}
          >
            {tCommon("actions.cancel")}
          </button>
          <button
            type="button"
            className="btn-primary"
            onClick={handleSave}
            disabled={saving || loading}
          >
            {saving ? tCommon("actions.saving") : tCommon("actions.save")}
          </button>
        </div>
      </div>
    </div>
  );
}

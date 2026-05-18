import { useEffect, useRef, useState } from "react";
import { Link, Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../../contexts/AuthContext";
import { useSiteConfig } from "../../contexts/SiteConfigContext";
import {
  appendAlbumMedia,
  createAlbum,
  deleteAlbumMedia,
  fetchAlbumForEdit,
  fetchCategoriesForCompose,
  updateAlbum,
  updateAlbumMediaCaption,
} from "./api";
import Layout from "../../components/Layout";
import VisibilityPicker from "../../components/VisibilityPicker";

const ACCEPTED_IMAGE_MIME = ["image/jpeg", "image/png", "image/gif", "image/webp"];
const ACCEPTED_VIDEO_MIME = ["video/mp4", "video/webm"];
const ACCEPTED_MIME = [...ACCEPTED_IMAGE_MIME, ...ACCEPTED_VIDEO_MIME];
const MAX_MEDIA = 50;
const MAX_IMAGE_BYTES = 10 * 1024 * 1024;
const MAX_VIDEO_BYTES = 100 * 1024 * 1024;
function maxBytesFor(mime) {
  return ACCEPTED_VIDEO_MIME.includes(mime) ? MAX_VIDEO_BYTES : MAX_IMAGE_BYTES;
}

/// `/compose-album` (and `/compose-album?edit={id}`). Author/admin only.
///
/// Create flow: POST /api/albums multipart with `name`, `description`,
/// `visibility`, `media[]` (files), and `caption_<index>` text fields for
/// per-item captions.
///
/// Edit flow: loads an existing album via GET /api/albums/{id}, exposes
/// PATCH /api/albums/{id} for metadata edits + POST /api/albums/{id}/media
/// for adding new media + DELETE /api/albums/{id}/media/{media_id} for
/// removing items + PATCH /api/albums/{id}/media/{media_id} for caption
/// edits. Each operation is its own request  there's no all-at-once
/// PATCH for an album in edit mode.
export default function ComposeAlbum() {
  const { user, loading: authLoading } = useAuth();
  const { config: site } = useSiteConfig();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const editId = searchParams.get("edit");
  const { t } = useTranslation("compose");
  const { t: tCommon } = useTranslation("common");

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [visibility, setVisibility] = useState("private");
  const [categoryId, setCategoryId] = useState("");
  const [categories, setCategories] = useState([]);
  // For create flow only  pending unsaved files with optional captions.
  // Shape: [{ file, previewUrl, caption }]
  const [pendingFiles, setPendingFiles] = useState([]);
  // For edit flow  the existing media items with their caption state.
  // Shape: [{ id, url, media_kind, caption, dirty }]
  const [existingMedia, setExistingMedia] = useState([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");
  const [loadingExisting, setLoadingExisting] = useState(Boolean(editId));
  const [dragActive, setDragActive] = useState(false);
  const fileInputRef = useRef(null);

  // Edit-mode load.
  useEffect(() => {
    let cancelled = false;
    async function loadCategories() {
      try {
        const response = await fetchCategoriesForCompose();
        if (response.ok) {
          const data = await response.json();
          if (!cancelled) setCategories(data.categories || []);
        }
      } catch {
        // not fatal  picker just hides
      }
    }
    loadCategories();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!editId) return;
    let cancelled = false;
    async function load() {
      try {
        const response = await fetchAlbumForEdit(editId);
        if (!response.ok) throw new Error(t("album.loadFailed"));
        const data = await response.json();
        if (!cancelled) {
          setName(data.name);
          setDescription(data.description || "");
          setVisibility(data.visibility);
          // The album response doesn't currently expose `category` like
          // posts do; fall back to the raw category_id when present.
          setCategoryId(data.category_id || "");
          setExistingMedia(
            data.media.map((m) => ({
              id: m.id,
              url: m.url,
              media_kind: m.media_kind,
              caption: m.caption || "",
              dirty: false,
            })),
          );
        }
      } catch (err) {
        if (!cancelled) setError(err.message);
      } finally {
        if (!cancelled) setLoadingExisting(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [editId]);

  // Revoke object URLs for pending files on unmount/clear.
  useEffect(() => {
    return () => {
      pendingFiles.forEach((f) => URL.revokeObjectURL(f.previewUrl));
    };
  }, []);

  if (!authLoading && (!user || (user.role !== "administrator" && user.role !== "poster"))) {
    return <Navigate to="/" replace />;
  }
  if (authLoading || loadingExisting) {
    return (
      <Layout>
        <p className="muted">{tCommon("loading")}</p>
      </Layout>
    );
  }
  // Posters lose access if poster_posting_enabled flips off mid-flight.
  if (
    user.role === "poster" &&
    (site === null || site.poster_posting_enabled !== true)
  ) {
    return (
      <Layout>
        <div className="alert alert-error" role="alert">
          {t("album.disabledShort")}
        </div>
      </Layout>
    );
  }

  function addFiles(incoming) {
    setError("");
    const remaining = MAX_MEDIA - existingMedia.length - pendingFiles.length;
    const accepted = [];
    for (const f of incoming) {
      if (accepted.length >= remaining) {
        setError(t("album.errorTooMany", { max: MAX_MEDIA }));
        break;
      }
      if (!ACCEPTED_MIME.includes(f.type)) {
        setError(t("attachments.errorUnsupported", { type: f.type }));
        continue;
      }
      const cap = maxBytesFor(f.type);
      if (f.size > cap) {
        const isVideo = ACCEPTED_VIDEO_MIME.includes(f.type);
        setError(
          t(
            isVideo
              ? "attachments.errorTooLargeAlbumVideo"
              : "attachments.errorTooLargeAlbumImage",
            { name: f.name },
          ),
        );
        continue;
      }
      accepted.push({
        file: f,
        previewUrl: URL.createObjectURL(f),
        caption: "",
      });
    }
    setPendingFiles((prev) => [...prev, ...accepted]);
  }

  function removePending(idx) {
    setPendingFiles((prev) => {
      const next = [...prev];
      const [removed] = next.splice(idx, 1);
      if (removed) URL.revokeObjectURL(removed.previewUrl);
      return next;
    });
  }

  function setPendingCaption(idx, caption) {
    setPendingFiles((prev) => {
      const next = [...prev];
      if (next[idx]) next[idx] = { ...next[idx], caption };
      return next;
    });
  }

  function setExistingCaption(idx, caption) {
    setExistingMedia((prev) => {
      const next = [...prev];
      if (next[idx]) next[idx] = { ...next[idx], caption, dirty: true };
      return next;
    });
  }

  async function deleteExistingMedia(idx) {
    const target = existingMedia[idx];
    if (!target) return;
    if (!window.confirm(t("album.removeItemConfirm"))) return;
    try {
      const response = await deleteAlbumMedia(editId, target.id);
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("album.removeItemFailed"));
      }
      setExistingMedia((prev) => prev.filter((_, i) => i !== idx));
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");
    setSuccess("");
    if (!name.trim()) {
      setError(t("album.nameRequired"));
      return;
    }
    setSubmitting(true);
    try {
      if (!editId) {
        // Create: one multipart request with everything.
        const form = new FormData();
        form.append("name", name);
        if (description.trim()) form.append("description", description);
        form.append("visibility", visibility);
        if (categoryId) form.append("category_id", categoryId);
        pendingFiles.forEach((f, i) => {
          form.append("media", f.file, f.file.name);
          if (f.caption.trim()) form.append(`caption_${i}`, f.caption);
        });
        const response = await createAlbum(form);
        if (!response.ok) {
          const data = await response.json().catch(() => ({}));
          throw new Error(data.error || t("album.createFailed"));
        }
        const data = await response.json();
        navigate(`/album/${data.id}`, { replace: true });
        return;
      }
      // Edit: stage of patches.
      // 1) PATCH metadata (name/desc/visibility/category) if any changed.
      const patchBody: { name: string; description: string; visibility: string; category_id?: string; clear_category?: boolean } = {
        name,
        description: description,
        visibility,
      };
      if (categoryId) {
        patchBody.category_id = categoryId;
      } else {
        patchBody.clear_category = true;
      }
      const patchResp = await updateAlbum(editId, patchBody);
      if (!patchResp.ok) {
        const data = await patchResp.json().catch(() => ({}));
        throw new Error(data.error || t("album.updateFailed"));
      }
      // 2) PATCH dirty caption changes one at a time.
      for (const m of existingMedia) {
        if (!m.dirty) continue;
        const r = await updateAlbumMediaCaption(editId, m.id, m.caption);
        if (!r.ok) {
          const data = await r.json().catch(() => ({}));
          throw new Error(data.error || t("album.captionUpdateFailed"));
        }
      }
      // 3) POST new media if any pendingFiles exist.
      if (pendingFiles.length > 0) {
        const form = new FormData();
        pendingFiles.forEach((f, i) => {
          form.append("media", f.file, f.file.name);
          if (f.caption.trim()) form.append(`caption_${i}`, f.caption);
        });
        const r = await appendAlbumMedia(editId, form);
        if (!r.ok) {
          const data = await r.json().catch(() => ({}));
          throw new Error(data.error || t("album.uploadMoreFailed"));
        }
      }
      navigate(`/album/${editId}`, { replace: true });
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  function onDrop(e) {
    e.preventDefault();
    setDragActive(false);
    if (e.dataTransfer?.files?.length) {
      addFiles(Array.from(e.dataTransfer.files));
    }
  }

  return (
    <Layout>
      <h1 className="sr-only">
        {editId ? t("album.editTitle") : t("album.newTitle")}
      </h1>
      <Link to="/" className="post-back-link">
        {t("backToFeed")}
      </Link>

      <form
        className="compose-card"
        onSubmit={handleSubmit}
        aria-busy={submitting}
        aria-labelledby="album-compose-title"
      >
        <h2 id="album-compose-title" className="compose-title">
          {editId ? t("album.editTitle") : t("album.newTitle")}
        </h2>

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

        <div className="compose-field">
          <label htmlFor="album-name">{t("album.nameLabel")}</label>
          <input
            id="album-name"
            name="name"
            type="text"
            autoComplete="off"
            className="compose-input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            maxLength={150}
            placeholder={t("album.namePlaceholder")}
          />
        </div>

        <div className="compose-field">
          <label htmlFor="album-description">{t("album.descLabel")}</label>
          <textarea
            id="album-description"
            name="description"
            className="compose-textarea-short"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
            maxLength={1000}
            placeholder={t("album.descPlaceholder")}
          />
        </div>

        {categoryId ? (
          <CategoryDrivenVisibilityNote
            category={categories.find((c) => c.id === categoryId)}
          />
        ) : (
          <VisibilityPicker value={visibility} onChange={setVisibility} />
        )}

        <div className="compose-field">
          <label htmlFor="album-category">{t("category.label")}</label>
          <select
            id="album-category"
            name="category_id"
            className="compose-input"
            value={categoryId}
            onChange={(e) => setCategoryId(e.target.value)}
          >
            <option value="">{t("category.uncategorized")}</option>
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
          {categories.length === 0 && (
            <p className="form-hint">{t("category.empty")}</p>
          )}
        </div>

        {existingMedia.length > 0 && (
          <div className="compose-field">
            <label>
              {t("album.existingItems", { count: existingMedia.length })}
            </label>
            <div className="album-edit-grid">
              {existingMedia.map((m, idx) => (
                <div key={m.id} className="album-edit-item">
                  <div className="album-edit-thumb">
                    {m.media_kind === "video" ? (
                      <video src={m.url} muted playsInline preload="metadata">
                        <track default kind="captions" srcLang="en" src="data:text/vtt;base64,V0VCVlRUCgo=" />
                        <track kind="descriptions" srcLang="en" src="data:text/vtt;base64,V0VCVlRUCgo=" />
                      </video>
                    ) : (
                      <img src={m.url} alt={m.caption || ""} />
                    )}
                  </div>
                  <input
                    type="text"
                    name={`existing-caption-${m.id}`}
                    aria-label={t("album.captionPlaceholder")}
                    className="album-edit-caption"
                    value={m.caption}
                    onChange={(e) => setExistingCaption(idx, e.target.value)}
                    placeholder={t("album.captionPlaceholder")}
                    maxLength={500}
                  />
                  <button
                    type="button"
                    className="btn-secondary album-edit-remove"
                    onClick={() => deleteExistingMedia(idx)}
                  >
                    {tCommon("actions.remove")}
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="compose-field">
          <label>{editId ? t("album.addMore") : t("album.items")}</label>
          <div
            className={`dropzone ${dragActive ? "drag-active" : ""}`}
            onDragEnter={(e) => {
              e.preventDefault();
              setDragActive(true);
            }}
            onDragOver={(e) => {
              e.preventDefault();
              setDragActive(true);
            }}
            onDragLeave={() => setDragActive(false)}
            onDrop={onDrop}
            onClick={() => fileInputRef.current?.click()}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                fileInputRef.current?.click();
              }
            }}
            role="button"
            tabIndex={0}
            aria-label={t("attachments.dropzoneAria")}
          >
            <p className="dropzone-prompt">
              {t("attachments.dropzonePrompt")}
            </p>
            <p className="dropzone-meta">
              {t("attachments.dropzoneMetaAlbum", { max: MAX_MEDIA })}
            </p>
            <input
              ref={fileInputRef}
              type="file"
              name="album-items"
              aria-label={t("attachments.dropzoneAria")}
              multiple
              accept={ACCEPTED_MIME.join(",")}
              style={{ display: "none" }}
              onChange={(e) => {
                if (e.target.files) addFiles(Array.from(e.target.files));
                e.target.value = "";
              }}
            />
          </div>

          {pendingFiles.length > 0 && (
            <div className="album-edit-grid album-edit-grid-pending">
              {pendingFiles.map((f, i) => (
                <div key={i} className="album-edit-item">
                  <div className="album-edit-thumb">
                    {ACCEPTED_VIDEO_MIME.includes(f.file.type) ? (
                      <video
                        src={f.previewUrl}
                        muted
                        playsInline
                        preload="metadata"
                      >
                        <track default kind="captions" srcLang="en" src="data:text/vtt;base64,V0VCVlRUCgo=" />
                        <track kind="descriptions" srcLang="en" src="data:text/vtt;base64,V0VCVlRUCgo=" />
                      </video>
                    ) : (
                      <img src={f.previewUrl} alt={f.file.name} />
                    )}
                  </div>
                  <input
                    type="text"
                    name={`pending-caption-${i}`}
                    aria-label={t("album.captionPlaceholder")}
                    className="album-edit-caption"
                    value={f.caption}
                    onChange={(e) => setPendingCaption(i, e.target.value)}
                    placeholder={t("album.captionPlaceholder")}
                    maxLength={500}
                  />
                  <button
                    type="button"
                    className="btn-secondary album-edit-remove"
                    onClick={() => removePending(i)}
                  >
                    {tCommon("actions.remove")}
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="compose-footer">
          <span className="form-hint">
            {editId ? t("album.editHint") : t("album.publishHint")}
          </span>
          <div className="compose-footer-actions">
            <Link
              to={editId ? `/album/${editId}` : "/"}
              className="btn-secondary"
            >
              {tCommon("actions.cancel")}
            </Link>
            <button
              type="submit"
              className="btn-primary"
              disabled={submitting || !name.trim()}
            >
              {submitting
                ? tCommon("actions.saving")
                : editId
                  ? t("post.saveCta")
                  : t("album.createCta")}
            </button>
          </div>
        </div>
      </form>
    </Layout>
  );
}

/// Stand-in for `<VisibilityPicker>` when a category is selected on an
/// album. The category drives the album's effective visibility.
function CategoryDrivenVisibilityNote({ category }) {
  const { t: tFeed } = useTranslation("feed");
  const { t: tCompose } = useTranslation("compose");
  if (!category) return null;
  const known = ["private", "commenters", "posters", "user_list"];
  const key = known.includes(category.visibility)
    ? category.visibility
    : "public";
  return (
    <div className="compose-field">
      <label>{tCompose("visibility.legend")}</label>
      <p className="form-hint">
        {tFeed(`visibility.${key}.name`)}{" "}
        <span className="muted">({tFeed("visibility.fromCategory")})</span>
      </p>
    </div>
  );
}

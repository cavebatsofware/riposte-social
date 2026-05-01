import { useEffect, useRef, useState } from "react";
import { Link, Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { useSiteConfig } from "../contexts/SiteConfigContext";
import { fetchApi } from "../utils/api";
import Layout from "../components/Layout";
import VisibilityPicker from "../components/VisibilityPicker";

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
/// edits. Each operation is its own request — there's no all-at-once
/// PATCH for an album in edit mode.
export default function ComposeAlbum() {
  const { user, loading: authLoading } = useAuth();
  const { config: site } = useSiteConfig();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const editId = searchParams.get("edit");

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [visibility, setVisibility] = useState("private");
  // For create flow only — pending unsaved files with optional captions.
  // Shape: [{ file, previewUrl, caption }]
  const [pendingFiles, setPendingFiles] = useState([]);
  // For edit flow — the existing media items with their caption state.
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
    if (!editId) return;
    let cancelled = false;
    async function load() {
      try {
        const response = await fetchApi(`/api/albums/${editId}`);
        if (!response.ok) throw new Error("Failed to load album for editing");
        const data = await response.json();
        if (!cancelled) {
          setName(data.name);
          setDescription(data.description || "");
          setVisibility(data.visibility);
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!authLoading && (!user || (user.role !== "administrator" && user.role !== "poster"))) {
    return <Navigate to="/" replace />;
  }
  if (authLoading || loadingExisting) {
    return (
      <Layout>
        <p className="muted">Loading…</p>
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
        <div className="alert alert-error">Album creation is currently disabled.</div>
      </Layout>
    );
  }

  function addFiles(incoming) {
    setError("");
    const remaining = MAX_MEDIA - existingMedia.length - pendingFiles.length;
    const accepted = [];
    for (const f of incoming) {
      if (accepted.length >= remaining) {
        setError(`At most ${MAX_MEDIA} items per album.`);
        break;
      }
      if (!ACCEPTED_MIME.includes(f.type)) {
        setError(
          `Unsupported file type '${f.type}'. Images (JPEG/PNG/GIF/WebP) or videos (MP4/WebM).`,
        );
        continue;
      }
      const cap = maxBytesFor(f.type);
      if (f.size > cap) {
        const isVideo = ACCEPTED_VIDEO_MIME.includes(f.type);
        setError(
          `File '${f.name}' exceeds the ${isVideo ? "100 MB" : "10 MB"} per-file limit.`,
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
    if (!window.confirm("Remove this item from the album?")) return;
    try {
      const response = await fetchApi(
        `/api/albums/${editId}/media/${target.id}`,
        { method: "DELETE" },
      );
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to remove item");
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
      setError("Name is required.");
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
        pendingFiles.forEach((f, i) => {
          form.append("media", f.file, f.file.name);
          if (f.caption.trim()) form.append(`caption_${i}`, f.caption);
        });
        const response = await fetchApi("/api/albums", {
          method: "POST",
          body: form,
        });
        if (!response.ok) {
          const data = await response.json().catch(() => ({}));
          throw new Error(data.error || "Failed to create album");
        }
        const data = await response.json();
        navigate(`/album/${data.id}`, { replace: true });
        return;
      }
      // Edit: stage of patches.
      // 1) PATCH metadata (name/desc/visibility) if any changed.
      const patchBody = {
        name,
        description: description,
        visibility,
      };
      const patchResp = await fetchApi(`/api/albums/${editId}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(patchBody),
      });
      if (!patchResp.ok) {
        const data = await patchResp.json().catch(() => ({}));
        throw new Error(data.error || "Failed to update album");
      }
      // 2) PATCH dirty caption changes one at a time.
      for (const m of existingMedia) {
        if (!m.dirty) continue;
        const r = await fetchApi(
          `/api/albums/${editId}/media/${m.id}`,
          {
            method: "PATCH",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ caption: m.caption }),
          },
        );
        if (!r.ok) {
          const data = await r.json().catch(() => ({}));
          throw new Error(data.error || "Failed to update caption");
        }
      }
      // 3) POST new media if any pendingFiles exist.
      if (pendingFiles.length > 0) {
        const form = new FormData();
        pendingFiles.forEach((f, i) => {
          form.append("media", f.file, f.file.name);
          if (f.caption.trim()) form.append(`caption_${i}`, f.caption);
        });
        const r = await fetchApi(`/api/albums/${editId}/media`, {
          method: "POST",
          body: form,
        });
        if (!r.ok) {
          const data = await r.json().catch(() => ({}));
          throw new Error(data.error || "Failed to upload new media");
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
      <Link to="/" className="post-back-link">
        ← Back to feed
      </Link>

      <form className="compose-card" onSubmit={handleSubmit}>
        <h2 className="compose-title">{editId ? "Edit album" : "New album"}</h2>

        {error && <div className="alert alert-error">{error}</div>}
        {success && <div className="alert alert-success">{success}</div>}

        <div className="compose-field">
          <label htmlFor="album-name">Name</label>
          <input
            id="album-name"
            type="text"
            className="compose-input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            maxLength={150}
            placeholder="Album name"
          />
        </div>

        <div className="compose-field">
          <label htmlFor="album-description">Description (optional)</label>
          <textarea
            id="album-description"
            className="compose-textarea-short"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
            maxLength={1000}
            placeholder="A few words about this album."
          />
        </div>

        <VisibilityPicker value={visibility} onChange={setVisibility} />

        {existingMedia.length > 0 && (
          <div className="compose-field">
            <label>Existing items ({existingMedia.length})</label>
            <div className="album-edit-grid">
              {existingMedia.map((m, idx) => (
                <div key={m.id} className="album-edit-item">
                  <div className="album-edit-thumb">
                    {m.media_kind === "video" ? (
                      <video src={m.url} muted playsInline preload="metadata" />
                    ) : (
                      <img src={m.url} alt={m.caption || ""} />
                    )}
                  </div>
                  <input
                    type="text"
                    className="album-edit-caption"
                    value={m.caption}
                    onChange={(e) => setExistingCaption(idx, e.target.value)}
                    placeholder="Caption"
                    maxLength={500}
                  />
                  <button
                    type="button"
                    className="btn-secondary album-edit-remove"
                    onClick={() => deleteExistingMedia(idx)}
                  >
                    Remove
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="compose-field">
          <label>{editId ? "Add more items" : "Items"}</label>
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
          >
            <p className="dropzone-prompt">
              Drop images or videos here, or click to browse
            </p>
            <p className="dropzone-meta">
              Up to {MAX_MEDIA} items per album · 10 MB image / 100 MB video each
            </p>
            <input
              ref={fileInputRef}
              type="file"
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
                      />
                    ) : (
                      <img src={f.previewUrl} alt={f.file.name} />
                    )}
                  </div>
                  <input
                    type="text"
                    className="album-edit-caption"
                    value={f.caption}
                    onChange={(e) => setPendingCaption(i, e.target.value)}
                    placeholder="Caption"
                    maxLength={500}
                  />
                  <button
                    type="button"
                    className="btn-secondary album-edit-remove"
                    onClick={() => removePending(i)}
                  >
                    Remove
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="compose-footer">
          <span className="form-hint">
            {editId
              ? "Saving will update the album immediately."
              : "Saving will publish immediately."}
          </span>
          <div className="compose-footer-actions">
            <Link to={editId ? `/album/${editId}` : "/"} className="btn-secondary">
              Cancel
            </Link>
            <button
              type="submit"
              className="btn-primary"
              disabled={submitting || !name.trim()}
            >
              {submitting ? "Saving…" : editId ? "Save changes" : "Create album"}
            </button>
          </div>
        </div>
      </form>
    </Layout>
  );
}

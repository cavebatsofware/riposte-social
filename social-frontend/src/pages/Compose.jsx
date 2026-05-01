import { useEffect, useMemo, useRef, useState } from "react";
import { Link, Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { useAuth } from "../contexts/AuthContext";
import { useSiteConfig } from "../contexts/SiteConfigContext";
import { fetchApi } from "../utils/api";
import Layout from "../components/Layout";

const VISIBILITIES = [
  { id: "private", label: "Private", desc: "Only you can see this" },
  { id: "public", label: "Public", desc: "Anyone with the link" },
  { id: "commenters", label: "Commenters", desc: "Friends with invites" },
  { id: "posters", label: "Posters", desc: "Family authors only" },
];

const ACCEPTED_IMAGE_MIME = ["image/jpeg", "image/png", "image/gif", "image/webp"];
const ACCEPTED_VIDEO_MIME = ["video/mp4", "video/webm"];
const ACCEPTED_MIME = [...ACCEPTED_IMAGE_MIME, ...ACCEPTED_VIDEO_MIME];
const MAX_MEDIA = 8;
const MAX_IMAGE_BYTES = 10 * 1024 * 1024;
const MAX_VIDEO_BYTES = 100 * 1024 * 1024;
function maxBytesFor(mime) {
  return ACCEPTED_VIDEO_MIME.includes(mime) ? MAX_VIDEO_BYTES : MAX_IMAGE_BYTES;
}

/// Compose page at `/compose`. Gated to admin/poster (commenters and
/// anonymous visitors are bounced to /). Multipart submission to
/// `POST /api/posts`; the backend stores body, visibility, and uploaded
/// media in one transaction (see `src/posts/routes.rs::create_post`).
///
/// The live preview pipes the markdown source through `marked` and then
/// DOMPurify to sanitize before injection, mirroring the server's
/// pulldown-cmark + ammonia pipeline. The preview is approximate; the
/// server's render is authoritative.
///
/// Edit mode (`?edit={id}`) loads an existing post and PATCHes on submit.
/// Media isn't editable in this first pass: edits keep the existing
/// attachments. To swap attachments, delete and re-create the post.
export default function Compose() {
  const { user, loading: authLoading } = useAuth();
  const { config: site } = useSiteConfig();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const editId = searchParams.get("edit");

  const [body, setBody] = useState("");
  // New posts default to "private" — author can promote at compose time
  // or with the quick-toggle on the feed card (Phase 9b). Edit mode
  // overwrites this with the post's actual visibility on load.
  const [visibility, setVisibility] = useState("private");
  const [files, setFiles] = useState([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [loadingExisting, setLoadingExisting] = useState(Boolean(editId));
  const [dragActive, setDragActive] = useState(false);
  const fileInputRef = useRef(null);

  useEffect(() => {
    if (!editId) return;
    let cancelled = false;
    async function load() {
      try {
        const response = await fetchApi(`/api/posts/${editId}`);
        if (!response.ok) throw new Error("Failed to load post for editing");
        const data = await response.json();
        if (!cancelled) {
          setBody(data.body);
          setVisibility(data.visibility);
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

  useEffect(() => {
    return () => {
      files.forEach((f) => URL.revokeObjectURL(f.previewUrl));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Sanitized HTML for the live preview. DOMPurify strips scripts,
  // event handlers, and javascript: URLs even when the author types them
  // intentionally to test, matching the server's ammonia allowlist.
  const previewHtml = useMemo(() => {
    if (!body.trim()) {
      return '<p class="muted">Live preview will appear here.</p>';
    }
    const raw = marked.parse(body, { breaks: true, gfm: true });
    return DOMPurify.sanitize(raw);
  }, [body]);

  if (authLoading) {
    return (
      <Layout>
        <p>Loading…</p>
      </Layout>
    );
  }
  if (!user) {
    return <Navigate to="/login" replace />;
  }
  if (user.role !== "administrator" && user.role !== "poster") {
    return <Navigate to="/" replace />;
  }
  // Posters can be muted by an admin without revoking their role. When the
  // gate is off the backend rejects POST anyway; surfacing a friendly
  // notice here saves a confusing round-trip.
  //
  // Fail closed: until /api/site/config returns the gate explicitly true,
  // refuse to render the form. A transient settings outage shouldn't let
  // a poster paste in their post and then fail at submit.
  if (
    user.role === "poster" &&
    !(site !== null && site.poster_posting_enabled === true)
  ) {
    return (
      <Layout>
        <Link to="/" className="post-back-link">
          ← Back to feed
        </Link>
        <section className="feed-empty">
          <p>
            Posting has been temporarily disabled by an administrator.
            Existing posts are unaffected.
          </p>
        </section>
      </Layout>
    );
  }

  function addFiles(incoming) {
    setError("");
    const accepted = [];
    let remaining = MAX_MEDIA - files.length;
    for (const f of incoming) {
      if (remaining <= 0) {
        setError(`At most ${MAX_MEDIA} attachments per post.`);
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
          `File '${f.name}' exceeds the ${isVideo ? "100 MB" : "10 MB"} per-file limit for ${isVideo ? "video" : "image"} content.`,
        );
        continue;
      }
      accepted.push({ file: f, previewUrl: URL.createObjectURL(f) });
      remaining -= 1;
    }
    setFiles((prev) => [...prev, ...accepted]);
  }

  function removeFile(idx) {
    setFiles((prev) => {
      const next = [...prev];
      const [removed] = next.splice(idx, 1);
      if (removed) URL.revokeObjectURL(removed.previewUrl);
      return next;
    });
  }

  function handleDragOver(e) {
    e.preventDefault();
    setDragActive(true);
  }
  function handleDragLeave() {
    setDragActive(false);
  }
  function handleDrop(e) {
    e.preventDefault();
    setDragActive(false);
    if (editId) return;
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      addFiles(Array.from(e.dataTransfer.files));
    }
  }

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");
    if (!body.trim()) {
      setError("Post body cannot be empty.");
      return;
    }
    setSubmitting(true);
    try {
      let response;
      if (editId) {
        response = await fetchApi(`/api/posts/${editId}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ body, visibility }),
        });
      } else {
        const form = new FormData();
        form.append("body", body);
        form.append("visibility", visibility);
        for (const { file } of files) {
          form.append("media", file);
        }
        response = await fetchApi("/api/posts", {
          method: "POST",
          body: form,
        });
      }
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to save post");
      }
      const saved = await response.json();
      navigate(`/post/${saved.id}`, { replace: true });
    } catch (err) {
      setError(err.message);
      setSubmitting(false);
    }
  }

  if (loadingExisting) {
    return (
      <Layout>
        <p>Loading post…</p>
      </Layout>
    );
  }

  return (
    <Layout>
      <Link to="/" className="post-back-link">
        ← Back to feed
      </Link>
      <p className="feed-subtitle">
        Composing as {user.display_name || user.email} ·{" "}
        {user.role === "administrator" ? "Administrator" : "Poster"}
      </p>

      <form className="compose-card" onSubmit={handleSubmit}>
        <h2 className="compose-title">{editId ? "Edit post" : "New post"}</h2>

        {error && <div className="alert alert-error">{error}</div>}

        <fieldset className="compose-field">
          <legend>Visibility</legend>
          <div className="visibility-options">
            {VISIBILITIES.map((v) => (
              <button
                key={v.id}
                type="button"
                className={`visibility-pill ${visibility === v.id ? "selected" : ""}`}
                onClick={() => setVisibility(v.id)}
              >
                <span className="visibility-pill-label">{v.label}</span>
                <span className="visibility-pill-desc">{v.desc}</span>
              </button>
            ))}
          </div>
        </fieldset>

        <div className="compose-field">
          <label htmlFor="compose-body">Body (markdown)</label>
          <textarea
            id="compose-body"
            className="compose-textarea"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder={
              "What's on your mind?\n\n**Bold**, *italic*, [links](https://example.com), and ![images](url) all work."
            }
          />
          <p className="form-hint">
            Markdown is rendered server-side. Inline images and links are
            first-class. HTML is sanitized.
          </p>
        </div>

        <div className="compose-field">
          <label>Preview</label>
          <PreviewPane html={previewHtml} />
        </div>

        {!editId && (
          <div className="compose-field">
            <label>Attachments</label>
            <div
              className={`dropzone ${dragActive ? "drag-active" : ""}`}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
              onClick={() => fileInputRef.current?.click()}
              role="button"
              tabIndex={0}
            >
              <p className="dropzone-prompt">
                Drop images or videos here, or click to browse
              </p>
              <p className="dropzone-meta">
                Images (JPEG/PNG/GIF/WebP, 10 MB each) or videos (MP4/WebM,
                100 MB each) · up to {MAX_MEDIA} files
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
            {files.length > 0 && (
              <div className="attached-row">
                {files.map((f, i) => (
                  <div key={i} className="attached-thumb">
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
                    <button
                      type="button"
                      className="attached-remove"
                      onClick={() => removeFile(i)}
                      aria-label={`Remove ${f.file.name}`}
                    >
                      ×
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        <div className="compose-footer">
          <span className="form-hint">
            {editId
              ? "Saving will update the post immediately."
              : "Saving will publish immediately."}
          </span>
          <div className="compose-footer-actions">
            <Link to="/" className="btn-secondary">
              Cancel
            </Link>
            <button
              type="submit"
              className="btn-primary"
              disabled={submitting || !body.trim()}
            >
              {submitting ? "Saving…" : editId ? "Save changes" : "Publish"}
            </button>
          </div>
        </div>
      </form>
    </Layout>
  );
}

/// Renders sanitized HTML for the markdown preview. Isolated into its own
/// component so the dangerouslySetInnerHTML usage is colocated with the
/// DOMPurify guarantee on the input.
function PreviewPane({ html }) {
  // `html` is always run through DOMPurify by the caller (see previewHtml
  // useMemo above). We re-sanitize here as a localized invariant: this
  // component never accepts unsanitized HTML, even if a future caller
  // forgets.
  const safe = DOMPurify.sanitize(html);
  return (
    <div
      className="preview-pane post-body"
      dangerouslySetInnerHTML={{ __html: safe }}
    />
  );
}

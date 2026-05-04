import { useEffect, useMemo, useRef, useState } from "react";
import { Link, Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { useAuth } from "../contexts/AuthContext";
import { useSiteConfig } from "../contexts/SiteConfigContext";
import { fetchApi } from "../utils/api";
import { LOCALE_NATIVE_NAMES, SUPPORTED_LOCALES } from "../i18n";
import {
  ftsConfigForLocale,
  SUPPORTED_CONTENT_LANGS,
} from "../utils/contentLang";
import Layout from "../components/Layout";
import VisibilityPicker from "../components/VisibilityPicker";

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
  const { t, i18n } = useTranslation("compose");
  const { t: tCommon } = useTranslation("common");

  const [body, setBody] = useState("");
  // New posts default to "private" — author can promote at compose time
  // or with the quick-toggle on the feed card (Phase 9b). Edit mode
  // overwrites this with the post's actual visibility on load.
  const [visibility, setVisibility] = useState("private");
  // Phase 9e: empty string means "uncategorized". On edit, hydrated from
  // the post's existing category_id. Categories list is fetched once on
  // mount; the dropdown shows the full set ordered by ordinal.
  const [categoryId, setCategoryId] = useState("");
  const [categories, setCategories] = useState([]);
  // FTS configuration the post will be tokenized under. Default derives
  // from the user's UI locale; edit mode overwrites with the post's
  // existing value on load.
  const [contentLang, setContentLang] = useState(() => {
    const active = (i18n.resolvedLanguage || i18n.language || "en").split(
      "-",
    )[0];
    return ftsConfigForLocale(active);
  });
  const [files, setFiles] = useState([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [loadingExisting, setLoadingExisting] = useState(Boolean(editId));
  const [dragActive, setDragActive] = useState(false);
  const fileInputRef = useRef(null);

  useEffect(() => {
    let cancelled = false;
    async function loadCategories() {
      try {
        const response = await fetchApi("/api/categories");
        if (response.ok) {
          const data = await response.json();
          if (!cancelled) setCategories(data.categories || []);
        }
      } catch {
        // Categories unavailable just hides the picker; not a hard error.
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
        const response = await fetchApi(`/api/posts/${editId}`);
        if (!response.ok) throw new Error(t("post.loadFailed"));
        const data = await response.json();
        if (!cancelled) {
          setBody(data.body);
          setVisibility(data.visibility);
          setCategoryId(data.category ? data.category.id : "");
          if (data.content_lang) setContentLang(data.content_lang);
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
      const empty = t("body.previewEmpty");
      return `<p class="muted">${empty}</p>`;
    }
    const raw = marked.parse(body, { breaks: true, gfm: true });
    return DOMPurify.sanitize(raw);
  }, [body, t]);

  if (authLoading) {
    return (
      <Layout>
        <p>{tCommon("loading")}</p>
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
          {t("backToFeed")}
        </Link>
        <section className="feed-empty">
          <p>{t("post.disabledNotice")}</p>
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
        setError(t("attachments.errorTooMany", { max: MAX_MEDIA }));
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
              ? "attachments.errorTooLargeVideo"
              : "attachments.errorTooLargeImage",
            { name: f.name },
          ),
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
      setError(t("post.bodyEmpty"));
      return;
    }
    setSubmitting(true);
    try {
      let response;
      if (editId) {
        // Phase 9e: send category_id when set, or `clear_category: true`
        // to drop an existing assignment (the empty-string option in the
        // picker becomes a clear).
        const patchBody = { body, visibility, content_lang: contentLang };
        if (categoryId) {
          patchBody.category_id = categoryId;
        } else {
          patchBody.clear_category = true;
        }
        response = await fetchApi(`/api/posts/${editId}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(patchBody),
        });
      } else {
        const form = new FormData();
        form.append("body", body);
        form.append("visibility", visibility);
        form.append("content_lang", contentLang);
        if (categoryId) form.append("category_id", categoryId);
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
        throw new Error(data.error || t("post.saveFailed"));
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
        <p>{t("post.loadingExisting")}</p>
      </Layout>
    );
  }

  const roleLabel =
    user.role === "administrator"
      ? t("role.administrator")
      : t("role.poster");

  return (
    <Layout>
      <Link to="/" className="post-back-link">
        {t("backToFeed")}
      </Link>
      <p className="feed-subtitle">
        {t("composingAs", {
          name: user.display_name || user.email,
          role: roleLabel,
        })}
      </p>

      <form className="compose-card" onSubmit={handleSubmit}>
        <h2 className="compose-title">
          {editId ? t("post.editTitle") : t("post.newTitle")}
        </h2>

        {error && <div className="alert alert-error">{error}</div>}

        <div className="compose-field">
          <label htmlFor="compose-body">{t("body.label")}</label>
          <textarea
            id="compose-body"
            className="compose-textarea"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder={t("body.placeholder")}
          />
          <p className="form-hint">{t("body.hint")}</p>
        </div>

        <div className="compose-field">
          <label>{t("body.previewLabel")}</label>
          <PreviewPane html={previewHtml} />
        </div>

        {categoryId ? (
          <CategoryDrivenVisibilityNote
            category={categories.find((c) => c.id === categoryId)}
          />
        ) : (
          <VisibilityPicker value={visibility} onChange={setVisibility} />
        )}

        <div className="compose-field-row">
          <div className="compose-field">
            <label htmlFor="compose-category">{t("category.label")}</label>
            <select
              id="compose-category"
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

          <div className="compose-field">
            <label htmlFor="compose-content-lang">
              {t("contentLang.label")}
            </label>
            <select
              id="compose-content-lang"
              className="compose-input"
              value={contentLang}
              onChange={(e) => setContentLang(e.target.value)}
            >
              {SUPPORTED_LOCALES.map((loc) => {
                const cfg = ftsConfigForLocale(loc);
                if (!SUPPORTED_CONTENT_LANGS.includes(cfg)) return null;
                return (
                  <option key={loc} value={cfg}>
                    {LOCALE_NATIVE_NAMES[loc]}
                  </option>
                );
              })}
            </select>
            <p className="form-hint">{t("contentLang.hint")}</p>
          </div>
        </div>

        {!editId && (
          <div className="compose-field">
            <label>{t("attachments.label")}</label>
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
                {t("attachments.dropzonePrompt")}
              </p>
              <p className="dropzone-meta">
                {t("attachments.dropzoneMeta", { max: MAX_MEDIA })}
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
                      aria-label={t("attachments.removeAria", { name: f.file.name })}
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
            {editId ? t("post.editHint") : t("post.publishHint")}
          </span>
          <div className="compose-footer-actions">
            <Link to="/" className="btn-secondary">
              {tCommon("actions.cancel")}
            </Link>
            <button
              type="submit"
              className="btn-primary"
              disabled={submitting || !body.trim()}
            >
              {submitting
                ? tCommon("actions.saving")
                : editId
                  ? t("post.saveCta")
                  : t("post.publishCta")}
            </button>
          </div>
        </div>
      </form>
    </Layout>
  );
}

/// Stand-in for `<VisibilityPicker>` when a category is selected. The
/// category drives the post's effective visibility, so we surface that
/// inline note instead of a picker the user can edit.
function CategoryDrivenVisibilityNote({ category }) {
  const { t } = useTranslation("feed");
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
        {t(`visibility.${key}.name`)}{" "}
        <span className="muted">({t("visibility.fromCategory")})</span>
      </p>
    </div>
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

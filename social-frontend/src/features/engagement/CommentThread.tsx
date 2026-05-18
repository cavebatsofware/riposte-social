import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import DOMPurify from "dompurify";
import { marked } from "marked";
import { useAuth } from "../../contexts/AuthContext";
import {
  createComment,
  deleteComment,
  editComment,
  fetchComments,
} from "./api";
import { formatRelativeTime } from "../../utils/formatTime";
import ReactionBar from "./ReactionBar";
// `.skeleton-line` classes are defined in SkeletonCard.css.
import "../../components/SkeletonCard.css";

/// Comment thread for a post permalink OR a media item inside a lightbox.
///
/// `target` selects the URL family:
///   `{ kind: "post", postId }` → `/api/posts/{postId}/comments`
///   `{ kind: "media", postId, mediaId }` →
///       `/api/posts/{postId}/media/{mediaId}/comments`
///
/// - Fetches the list on mount (and again when `target` changes, since the
///   lightbox swaps `mediaId` as the user pages through items).
/// - Authenticated callers get a textarea + Post button. Anonymous
///   callers see the list and a "Sign in to comment" pointer.
/// - Soft-deletion on the backend keeps moderated rows out of the
///   response, so we never have to render a placeholder for a removed
///   comment here.
/// - Body HTML is sanitized server-side (`ammonia`) and re-sanitized
///   client-side (`DOMPurify`) before injection. Defense-in-depth, same
///   as `<PostCard>`.
export default function CommentThread({ target }) {
  const { user } = useAuth();
  const { t } = useTranslation("feed");
  const [comments, setComments] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [draft, setDraft] = useState("");
  const [submitting, setSubmitting] = useState(false);

  // Stable URL key for the current target. Changing it (e.g. moving the
  // lightbox to a different media item) triggers refetch + clears the
  // composer draft so a half-typed reply doesn't leak across items.
  const baseUrl =
    target.kind === "media"
      ? `/api/posts/${target.postId}/media/${target.mediaId}/comments`
      : `/api/posts/${target.postId}/comments`;

  useEffect(() => {
    let cancelled = false;
    setDraft("");
    async function load() {
      setLoading(true);
      setError("");
      try {
        const response = await fetchComments(baseUrl);
        if (response.status === 401 || response.status === 404) {
          if (!cancelled) setComments([]);
          return;
        }
        if (!response.ok) throw new Error(t("comments.loadFailed"));
        const data = await response.json();
        if (!cancelled) setComments(data.comments || []);
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
  }, [baseUrl, t]);

  async function handleSubmit(e) {
    e.preventDefault();
    if (!draft.trim() || submitting) return;
    setSubmitting(true);
    setError("");
    try {
      const response = await createComment(baseUrl, draft);
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("comments.submitFailed"));
      }
      const created = await response.json();
      setComments((prev) => [...prev, created]);
      setDraft("");
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  async function handleDelete(commentId) {
    if (!window.confirm(t("comments.deleteConfirm"))) return;
    try {
      const response = await deleteComment(baseUrl, commentId);
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("comments.deleteFailed"));
      }
      setComments((prev) => prev.filter((c) => c.id !== commentId));
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleEdit(commentId, body) {
    const response = await editComment(baseUrl, commentId, body);
    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error(data.error || t("comments.editFailed"));
    }
    const updated = await response.json();
    setComments((prev) => prev.map((c) => (c.id === updated.id ? updated : c)));
  }

  const titleText =
    comments.length > 0
      ? t("comments.titleWithCount", { count: comments.length })
      : t("comments.title");

  return (
    <section className="comment-thread" aria-label={t("comments.title")}>
      <h3 className="comment-thread-title">{titleText}</h3>

      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}

      {loading ? (
        <ol
          className="comment-list"
          aria-busy="true"
          aria-label={t("comments.loadingAria")}
        >
          {Array.from({ length: 2 }).map((_, i) => (
            <li key={i} className="comment-item" aria-hidden="true">
              <header className="comment-item-meta">
                <span className="skeleton-line skeleton-line-md" />
              </header>
              <div className="comment-item-body">
                <span className="skeleton-line skeleton-line-full" />
                <span className="skeleton-line skeleton-line-3q" />
              </div>
            </li>
          ))}
        </ol>
      ) : comments.length === 0 ? (
        <p className="muted">{t("comments.empty")}</p>
      ) : (
        <ol className="comment-list">
          {comments.map((c) => (
            <CommentItem
              key={c.id}
              target={target}
              comment={c}
              viewer={user}
              onDelete={handleDelete}
              onEdit={handleEdit}
            />
          ))}
        </ol>
      )}

      {user ? (
        <form
          className="comment-compose"
          onSubmit={handleSubmit}
          aria-busy={submitting}
          aria-label={t("comments.compose.label")}
        >
          <label htmlFor="comment-draft" className="comment-compose-label">
            {t("comments.compose.label")}
          </label>
          <CommentMarkdownArea
            id="comment-draft"
            value={draft}
            onChange={setDraft}
            placeholder={t("comments.compose.placeholder")}
            rows={3}
            maxLength={4000}
          />
          <div className="comment-compose-actions">
            <span className="form-hint">
              {t("comments.compose.charCount", {
                current: draft.length,
                max: 4000,
              })}
            </span>
            <button
              type="submit"
              className="btn-primary"
              disabled={submitting || !draft.trim()}
            >
              {submitting
                ? t("comments.compose.submitting")
                : t("comments.compose.submit")}
            </button>
          </div>
        </form>
      ) : (
        <p className="comment-thread-signin muted">
          {t("comments.signInPrompt")}
        </p>
      )}
    </section>
  );
}

/// Single comment row. Body HTML is run through DOMPurify here as a
/// localized invariant: this component never accepts unsanitized HTML, even
/// if a future caller forgets the upstream sanitization step.
///
/// Inline ReactionBar renders only for post comments. Media-comment rows
/// (target.kind === "media") skip the bar; the backend has no
/// `/comments/{id}/reactions` route under the media path, and the
/// `MediaCommentResponse` payload doesn't carry the reaction-count
/// fields anyway.
function CommentItem({ target, comment, viewer, onDelete, onEdit }) {
  const { t, i18n } = useTranslation("feed");
  const { t: tCommon } = useTranslation("common");
  const author =
    comment.author_display || comment.author_handle || t("fallbackAuthor");
  const time = formatRelativeTime(comment.created_at, i18n.language);
  const safe = DOMPurify.sanitize(comment.body_html || "");
  const isAuthor = viewer && viewer.id === comment.user_id;
  const isAdmin = viewer && viewer.role === "administrator";
  const canEdit = isAuthor || isAdmin;
  const canDelete = isAuthor || isAdmin;
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(comment.body);
  const [saving, setSaving] = useState(false);
  const [editError, setEditError] = useState("");

  function startEdit() {
    setDraft(comment.body);
    setEditError("");
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    setEditError("");
  }

  async function saveEdit() {
    const next = draft.trim();
    if (!next || saving) return;
    if (next === comment.body.trim()) {
      setEditing(false);
      return;
    }
    setSaving(true);
    setEditError("");
    try {
      await onEdit(comment.id, next);
      setEditing(false);
    } catch (err) {
      setEditError(err.message);
    } finally {
      setSaving(false);
    }
  }

  const authorEl = comment.author_handle ? (
    <Link
      to={`/u/${comment.author_handle}`}
      className="comment-item-author comment-item-author-link"
    >
      {author}
    </Link>
  ) : (
    <span className="comment-item-author">{author}</span>
  );

  return (
    <li className="comment-item">
      <header className="comment-item-meta">
        {authorEl}
        <span className="comment-item-dot" aria-hidden="true" />
        <span className="comment-item-time">{time}</span>
        {comment.edited_at && (
          <span
            className="comment-item-edited muted"
            title={t("comments.editedTitle", {
              when: formatRelativeTime(comment.edited_at, i18n.language),
            })}
          >
            {t("comments.edited")}
          </span>
        )}
        {!editing && canEdit && (
          <button
            type="button"
            className="comment-item-action"
            onClick={startEdit}
            aria-label={t("comments.editAria")}
          >
            {tCommon("actions.edit")}
          </button>
        )}
        {!editing && canDelete && (
          <button
            type="button"
            className="comment-item-delete"
            onClick={() => onDelete(comment.id)}
            aria-label={t("comments.deleteAria")}
          >
            {tCommon("actions.delete")}
          </button>
        )}
      </header>
      {editing ? (
        <div className="comment-item-edit">
          {editError && (
            <div className="alert alert-error" role="alert">
              {editError}
            </div>
          )}
          <CommentMarkdownArea
            value={draft}
            onChange={setDraft}
            rows={3}
            maxLength={4000}
          />
          <div className="comment-compose-actions">
            <span className="form-hint">
              {t("comments.compose.charCount", {
                current: draft.length,
                max: 4000,
              })}
            </span>
            <div className="comment-item-edit-buttons">
              <button
                type="button"
                className="btn-secondary"
                onClick={cancelEdit}
                disabled={saving}
              >
                {tCommon("actions.cancel")}
              </button>
              <button
                type="button"
                className="btn-primary"
                onClick={saveEdit}
                disabled={saving || !draft.trim()}
              >
                {saving
                  ? tCommon("actions.saving")
                  : tCommon("actions.save")}
              </button>
            </div>
          </div>
        </div>
      ) : (
        <>
          <div
            className="comment-item-body post-body"
            dangerouslySetInnerHTML={{ __html: safe }}
          />
          {target.kind !== "media" && (
            <ReactionBar
              target={{
                kind: "comment",
                postId: target.postId,
                commentId: comment.id,
              }}
              state={comment}
              compact
            />
          )}
        </>
      )}
    </li>
  );
}

/// Edit/Preview-tabbed textarea used by both the new-comment composer and
/// the inline edit form on existing comments. Preview pipes the markdown
/// source through `marked` and DOMPurify, mirroring the Compose page's
/// approximate-but-safe live preview pipeline.
///
/// The tab buttons + content panels follow the WAI-ARIA tabs pattern:
/// only the active tab is in the tab order (`tabindex=0`); arrow keys
/// move both focus and selection across tabs; the rendered panel
/// references its tab via `aria-labelledby` and is itself focusable so
/// keyboard users can land in the panel after tabbing past the tabs.
function CommentMarkdownArea({
  id = "",
  value,
  onChange,
  rows = 3,
  maxLength = 4000,
  placeholder = "",
}: {
  id?: string;
  value: string;
  onChange: (v: string) => void;
  rows?: number;
  maxLength?: number;
  placeholder?: string;
}) {
  const { t } = useTranslation("feed");
  const [tab, setTab] = useState("write");
  const writeTabId = useId();
  const previewTabId = useId();
  const writePanelId = useId();
  const previewPanelId = useId();
  const writeTabRef = useRef(null);
  const previewTabRef = useRef(null);

  const previewHtml = useMemo(() => {
    if (!value || !value.trim()) {
      const empty = t("comments.editor.previewEmpty");
      return `<p class="muted">${empty}</p>`;
    }
    const raw = marked.parse(value, { breaks: true, gfm: true }) as string;
    return DOMPurify.sanitize(raw);
  }, [value, t]);

  function onTabKey(e, currentTab) {
    let nextTab;
    switch (e.key) {
      case "Home":
        nextTab = "write";
        break;
      case "End":
        nextTab = "preview";
        break;
      case "ArrowLeft":
        nextTab = currentTab === "preview" ? "write" : "preview";
        break;
      case "ArrowRight":
        nextTab = currentTab === "write" ? "preview" : "write";
        break;
      default:
        return;
    }
    e.preventDefault();
    setTab(nextTab);
    if (nextTab === "write") {
      writeTabRef.current?.focus();
    } else {
      previewTabRef.current?.focus();
    }
  }

  return (
    <div className="comment-md-area">
      <div
        className="comment-md-tabs"
        role="tablist"
        aria-label={t("comments.editor.modeAria")}
      >
        <button
          ref={writeTabRef}
          type="button"
          role="tab"
          id={writeTabId}
          aria-selected={tab === "write"}
          aria-controls={writePanelId}
          tabIndex={tab === "write" ? 0 : -1}
          className={`comment-md-tab ${tab === "write" ? "is-active" : ""}`}
          onClick={() => setTab("write")}
          onKeyDown={(e) => onTabKey(e, "write")}
        >
          {t("comments.editor.tabWrite")}
        </button>
        <button
          ref={previewTabRef}
          type="button"
          role="tab"
          id={previewTabId}
          aria-selected={tab === "preview"}
          aria-controls={previewPanelId}
          tabIndex={tab === "preview" ? 0 : -1}
          className={`comment-md-tab ${tab === "preview" ? "is-active" : ""}`}
          onClick={() => setTab("preview")}
          onKeyDown={(e) => onTabKey(e, "preview")}
        >
          {t("comments.editor.tabPreview")}
        </button>
      </div>
      {tab === "write" ? (
        <div
          role="tabpanel"
          id={writePanelId}
          aria-labelledby={writeTabId}
        >
          <textarea
            id={id}
            name="body"
            className="comment-compose-textarea"
            value={value}
            onChange={(e) => onChange(e.target.value)}
            rows={rows}
            maxLength={maxLength}
            placeholder={placeholder}
          />
        </div>
      ) : (
        <div
          role="tabpanel"
          id={previewPanelId}
          aria-labelledby={previewTabId}
          tabIndex={0}
          className="comment-md-preview post-body"
          dangerouslySetInnerHTML={{ __html: previewHtml }}
        />
      )}
    </div>
  );
}


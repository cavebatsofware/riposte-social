import { useEffect, useState } from "react";
import DOMPurify from "dompurify";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";

/// Comment thread for the post permalink page.
///
/// - Fetches `GET /api/posts/{id}/comments` on mount.
/// - Authenticated callers (commenter, poster, admin) get a textarea +
///   Post button. Anonymous callers see the list and a "Sign in to comment"
///   pointer.
/// - Soft-deletion on the backend keeps moderated rows out of the response,
///   so we never have to render a placeholder for a removed comment here.
/// - Body HTML is sanitized server-side (`ammonia`) and re-sanitized
///   client-side (`DOMPurify`) before injection. Defense-in-depth, same as
///   `<PostCard>`.
export default function CommentThread({ postId }) {
  const { user } = useAuth();
  const [comments, setComments] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [draft, setDraft] = useState("");
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setError("");
      try {
        const response = await fetchApi(`/api/posts/${postId}/comments`);
        if (response.status === 401 || response.status === 404) {
          if (!cancelled) setComments([]);
          return;
        }
        if (!response.ok) throw new Error("Failed to load comments");
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
  }, [postId]);

  async function handleSubmit(e) {
    e.preventDefault();
    if (!draft.trim() || submitting) return;
    setSubmitting(true);
    setError("");
    try {
      const response = await fetchApi(`/api/posts/${postId}/comments`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ body: draft }),
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to post comment");
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
    if (!window.confirm("Delete this comment? This can't be undone.")) return;
    try {
      const response = await fetchApi(
        `/api/posts/${postId}/comments/${commentId}`,
        { method: "DELETE" },
      );
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to delete comment");
      }
      setComments((prev) => prev.filter((c) => c.id !== commentId));
    } catch (err) {
      setError(err.message);
    }
  }

  return (
    <section className="comment-thread" aria-label="Comments">
      <h3 className="comment-thread-title">
        Comments {comments.length > 0 && `(${comments.length})`}
      </h3>

      {error && <div className="alert alert-error">{error}</div>}

      {loading ? (
        <p className="muted">Loading comments…</p>
      ) : comments.length === 0 ? (
        <p className="muted">No comments yet.</p>
      ) : (
        <ol className="comment-list">
          {comments.map((c) => (
            <CommentItem
              key={c.id}
              comment={c}
              viewer={user}
              onDelete={handleDelete}
            />
          ))}
        </ol>
      )}

      {user ? (
        <form className="comment-compose" onSubmit={handleSubmit}>
          <label htmlFor="comment-draft" className="comment-compose-label">
            Add a comment
          </label>
          <textarea
            id="comment-draft"
            className="comment-compose-textarea"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="Markdown is supported."
            rows={3}
            maxLength={4000}
          />
          <div className="comment-compose-actions">
            <span className="form-hint">{draft.length}/4000 characters</span>
            <button
              type="submit"
              className="btn-primary"
              disabled={submitting || !draft.trim()}
            >
              {submitting ? "Posting…" : "Post comment"}
            </button>
          </div>
        </form>
      ) : (
        <p className="comment-thread-signin muted">
          Sign in to join the conversation.
        </p>
      )}
    </section>
  );
}

/// Single comment row. Body HTML is run through DOMPurify here as a
/// localized invariant: this component never accepts unsanitized HTML, even
/// if a future caller forgets the upstream sanitization step.
function CommentItem({ comment, viewer, onDelete }) {
  const author = comment.author_display || "Member";
  const time = formatRelativeTime(comment.created_at);
  const safe = DOMPurify.sanitize(comment.body_html || "");
  const canDelete =
    viewer &&
    (viewer.id === comment.user_id || viewer.role === "administrator");

  return (
    <li className="comment-item">
      <header className="comment-item-meta">
        <span className="comment-item-author">{author}</span>
        <span className="comment-item-dot" aria-hidden="true" />
        <span className="comment-item-time">{time}</span>
        {canDelete && (
          <button
            type="button"
            className="comment-item-delete"
            onClick={() => onDelete(comment.id)}
            aria-label="Delete comment"
          >
            Delete
          </button>
        )}
      </header>
      <div
        className="comment-item-body post-body"
        dangerouslySetInnerHTML={{ __html: safe }}
      />
    </li>
  );
}

function formatRelativeTime(iso) {
  const d = new Date(iso);
  const now = new Date();
  const diffSec = Math.floor((now - d) / 1000);
  if (diffSec < 60) return "just now";
  if (diffSec < 3600) {
    const m = Math.floor(diffSec / 60);
    return `${m} minute${m === 1 ? "" : "s"} ago`;
  }
  if (diffSec < 86400) {
    const h = Math.floor(diffSec / 3600);
    return `${h} hour${h === 1 ? "" : "s"} ago`;
  }
  if (diffSec < 7 * 86400) {
    const days = Math.floor(diffSec / 86400);
    if (days === 1) return "yesterday";
    return `${days} days ago`;
  }
  return d.toLocaleDateString();
}

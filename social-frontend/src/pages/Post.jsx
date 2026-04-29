import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import PostCard from "../components/PostCard";

/// Permalink page at `/post/:id`. Renders a single post with the full-
/// width media variant, plus a "back to feed" link. Author or admin sees
/// edit + delete buttons; everyone else just sees the post.
///
/// Comments + reactions arrive in Phase 4; the action bar is intentionally
/// absent for now to avoid promising functionality that isn't wired up.
export default function Post() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { user, logout } = useAuth();
  const [post, setPost] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setError("");
      try {
        const response = await fetchApi(`/api/posts/${id}`);
        if (response.status === 404 || response.status === 401) {
          if (!cancelled) {
            setError("Post not found.");
            setPost(null);
          }
          return;
        }
        if (!response.ok) {
          throw new Error("Failed to load post");
        }
        const data = await response.json();
        if (!cancelled) {
          setPost(data);
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
  }, [id]);

  const isAuthorOrAdmin =
    user &&
    post &&
    (user.id === post.author_id || user.role === "administrator");

  async function handleDelete() {
    if (!window.confirm("Delete this post? This can't be undone.")) {
      return;
    }
    setDeleting(true);
    setError("");
    try {
      const response = await fetchApi(`/api/posts/${id}`, { method: "DELETE" });
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to delete post");
      }
      navigate("/", { replace: true });
    } catch (err) {
      setError(err.message);
      setDeleting(false);
    }
  }

  return (
    <main className="feed">
      <header className="feed-header">
        <div className="feed-header-row">
          <Link to="/" className="post-back-link">
            ← Back to feed
          </Link>
          {user ? (
            <button type="button" className="btn-secondary" onClick={logout}>
              Sign out
            </button>
          ) : (
            <Link to="/login" className="btn-primary">
              Sign in
            </Link>
          )}
        </div>
      </header>

      {loading && (
        <section className="feed-empty">
          <p>Loading post…</p>
        </section>
      )}

      {!loading && error && <div className="alert alert-error">{error}</div>}

      {!loading && post && (
        <>
          <PostCard post={post} variant="permalink" />
          {isAuthorOrAdmin && (
            <div className="post-permalink-actions">
              <Link
                to={`/compose?edit=${post.id}`}
                className="btn-secondary"
              >
                Edit
              </Link>
              <button
                type="button"
                className="btn-secondary"
                onClick={handleDelete}
                disabled={deleting}
              >
                {deleting ? "Deleting…" : "Delete"}
              </button>
            </div>
          )}
          <section className="post-comments-placeholder">
            <p>Reactions and comments are coming in the next release.</p>
          </section>
        </>
      )}
    </main>
  );
}

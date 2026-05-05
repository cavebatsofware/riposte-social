import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import CommentThread from "../components/CommentThread";
import Layout from "../components/Layout";
import PostCard from "../components/PostCard";
import SkeletonCard from "../components/SkeletonCard";

/// Permalink page at `/post/:id`. Renders a single post with the full-
/// width media variant. Author or admin sees edit + delete buttons;
/// everyone else just sees the post.
///
/// Below the post, `<CommentThread />` lists existing comments and lets
/// authenticated callers add new ones. The reaction bar lives inside
/// `<PostCard />` so likes work from both feed and permalink without two
/// separate code paths.
///
/// Wrapped in `<Layout>` (Phase 7a). Page-local navigation has shrunk to
/// a single "Back to feed" link rendered above the post content.
export default function Post() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { user } = useAuth();
  const [post, setPost] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [deleting, setDeleting] = useState(false);
  const { t } = useTranslation("feed");
  const { t: tCommon } = useTranslation("common");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setError("");
      try {
        const response = await fetchApi(`/api/posts/${id}`);
        if (response.status === 404 || response.status === 401) {
          if (!cancelled) {
            setError(t("post.notFound"));
            setPost(null);
          }
          return;
        }
        if (!response.ok) {
          throw new Error(t("post.loadFailed"));
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
    if (!window.confirm(t("post.deleteConfirm"))) {
      return;
    }
    setDeleting(true);
    setError("");
    try {
      const response = await fetchApi(`/api/posts/${id}`, { method: "DELETE" });
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("post.deleteFailed"));
      }
      navigate("/", { replace: true });
    } catch (err) {
      setError(err.message);
      setDeleting(false);
    }
  }

  return (
    <Layout>
      <h1 className="sr-only">{t("post.heading")}</h1>
      <Link to="/" className="post-back-link">
        {tCommon("backToFeed")}
      </Link>

      {loading && <SkeletonCard />}

      {!loading && error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}

      {!loading && post && (
        <>
          <PostCard post={post} variant="permalink" />
          {isAuthorOrAdmin && (
            <div className="post-permalink-actions">
              <Link
                to={`/compose?edit=${post.id}`}
                className="btn-secondary"
              >
                {tCommon("actions.edit")}
              </Link>
              <button
                type="button"
                className="btn-secondary"
                onClick={handleDelete}
                disabled={deleting}
              >
                {deleting ? tCommon("actions.deleting") : tCommon("actions.delete")}
              </button>
            </div>
          )}
          <CommentThread postId={post.id} />
        </>
      )}
    </Layout>
  );
}

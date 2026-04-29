import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import InviteSplash from "../components/InviteSplash";
import PostCard from "../components/PostCard";

const FEED_LIMIT = 20;

/// Public feed at `/`. Anonymous visitors see public posts; commenters see
/// public + commenter-visible; posters and admins see everything. The
/// backend filters by tier (see `src/posts/routes.rs`); this component
/// just renders whatever it returns.
export default function Feed() {
  const { user, loading: authLoading, logout } = useAuth();
  const [posts, setPosts] = useState([]);
  const [cursor, setCursor] = useState(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const canCompose =
    user && (user.role === "administrator" || user.role === "poster");

  const loadPage = useCallback(
    async (nextCursor) => {
      setLoading(true);
      setError("");
      try {
        const url = nextCursor
          ? `/api/feed?limit=${FEED_LIMIT}&cursor=${encodeURIComponent(nextCursor)}`
          : `/api/feed?limit=${FEED_LIMIT}`;
        const response = await fetchApi(url);
        if (!response.ok) {
          throw new Error("Failed to load feed");
        }
        const data = await response.json();
        setPosts((prev) =>
          nextCursor ? [...prev, ...data.posts] : data.posts,
        );
        setCursor(data.next_cursor);
        setHasMore(Boolean(data.next_cursor));
      } catch (err) {
        setError(err.message);
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  useEffect(() => {
    loadPage(null);
  }, [loadPage]);

  return (
    <main className="feed">
      <header className="feed-header">
        <div className="feed-header-row">
          <h1>Riposte Social</h1>
          <div className="feed-header-actions">
            {!authLoading && canCompose && (
              <Link to="/compose" className="btn-primary">
                Compose
              </Link>
            )}
            {!authLoading &&
              (user ? (
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={logout}
                >
                  Sign out
                </button>
              ) : (
                <Link to="/login" className="btn-primary">
                  Sign in
                </Link>
              ))}
          </div>
        </div>
        <p className="feed-subtitle">
          {authLoading
            ? "…"
            : user
              ? `Signed in as ${user.display_name || user.email}`
              : "A self-hosted social site for family and close friends."}
        </p>
      </header>

      <InviteSplash />

      {error && <div className="alert alert-error">{error}</div>}

      {!loading && posts.length === 0 && !error && (
        <section className="feed-empty">
          <p>No posts yet. {canCompose && "Be the first to share something."}</p>
        </section>
      )}

      <section className="feed-list">
        {posts.map((p) => (
          <PostCard key={p.id} post={p} variant="feed" />
        ))}
      </section>

      {hasMore && (
        <div className="feed-load-more">
          <button
            type="button"
            className="btn-secondary"
            disabled={loading}
            onClick={() => loadPage(cursor)}
          >
            {loading ? "Loading…" : "Load more"}
          </button>
        </div>
      )}

      {loading && posts.length === 0 && (
        <section className="feed-empty">
          <p>Loading feed…</p>
        </section>
      )}
    </main>
  );
}

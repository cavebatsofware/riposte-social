import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { useSiteConfig } from "../contexts/SiteConfigContext";
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
  const { config: site } = useSiteConfig();
  const [posts, setPosts] = useState([]);
  const [cursor, setCursor] = useState(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  // Posting is gated by `poster_posting_enabled` for the poster role only;
  // admins always retain access. The site-config fetch starts as `null`
  // and only populates on success — until then we treat the gate as off
  // (fail closed) so a transient settings outage never causes us to
  // surface a Compose button that the backend would reject.
  const canCompose =
    user &&
    (user.role === "administrator" ||
      (user.role === "poster" &&
        site !== null &&
        site.poster_posting_enabled === true));

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
        <FeedEmptyState
          user={user}
          canCompose={canCompose}
          // Fail closed: until /api/site/config returns the gate
          // explicitly true, render the invite-only message rather than
          // the public-empty message.
          publicFeedEnabled={site !== null && site.public_feed_enabled === true}
        />
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

/// Feed empty-state with copy that adapts to the caller's tier and the
/// `public_feed_enabled` site mode.
///
/// - Anonymous + `!public_feed_enabled`: invite-only mode message + Sign in.
/// - Anonymous + `public_feed_enabled` + 0 posts: "nothing public yet" + Sign in.
/// - Authed admin/poster who can compose: Compose call-to-action.
/// - Authed commenter (or poster gated off): "no posts yet" silently.
function FeedEmptyState({ user, canCompose, publicFeedEnabled }) {
  if (!user) {
    if (!publicFeedEnabled) {
      return (
        <section className="feed-empty">
          <p>This site is invite-only.</p>
          <p className="feed-empty-hint">
            <Link to="/login">Sign in</Link> if you have an invite, or check
            back if a friend has shared one with you.
          </p>
        </section>
      );
    }
    return (
      <section className="feed-empty">
        <p>Nothing has been shared publicly yet.</p>
        <p className="feed-empty-hint">
          Check back later, or <Link to="/login">sign in</Link> if you have
          an invite.
        </p>
      </section>
    );
  }
  if (canCompose) {
    return (
      <section className="feed-empty">
        <p>No posts yet.</p>
        <p className="feed-empty-hint">
          <Link to="/compose">Compose something</Link> to get started.
        </p>
      </section>
    );
  }
  return (
    <section className="feed-empty">
      <p>No posts to show yet. Come back later.</p>
    </section>
  );
}

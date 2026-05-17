import { useCallback, useEffect, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Trans, useTranslation } from "react-i18next";
import { useAuth } from "../contexts/AuthContext";
import { useSiteConfig } from "../contexts/SiteConfigContext";
import { fetchApi } from "../utils/api";
import { recordCategoryVisit } from "../utils/browseHistory";
import InviteSplash from "../components/InviteSplash";
import Layout from "../components/Layout";
import PostCard from "../components/PostCard";
import SkeletonCard from "../components/SkeletonCard";

const FEED_LIMIT = 20;

/// Public feed at `/`. Anonymous visitors see public posts; commenters see
/// public + commenter-visible; posters and admins see everything. The
/// backend filters by tier (see `src/posts/routes.rs`); this component
/// just renders whatever it returns.
///
/// Wrapped in `<Layout>` so the header / nav / theme picker
/// come from the shared shell rather than per-page chrome.
export default function Feed() {
  const { user } = useAuth();
  const { config: site } = useSiteConfig();
  const [params, setParams] = useSearchParams();
  const category = params.get("category");
  const q = params.get("q") || "";
  const [posts, setPosts] = useState([]);
  const [cursor, setCursor] = useState(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const { t } = useTranslation("feed");
  const { t: tCommon } = useTranslation("common");

  // Local input state  committed to the URL only on submit so we don't
  // hammer the API on every keystroke.
  const [searchInput, setSearchInput] = useState(q);
  useEffect(() => {
    setSearchInput(q);
  }, [q]);

  // MRU bookkeeping: every category visit moves that category up the
  // BrowseRail's recency list. Server already filters posts by slug.
  useEffect(() => {
    if (category) recordCategoryVisit(category);
  }, [category]);

  // Posting is gated by `poster_posting_enabled` for the poster role only;
  // admins always retain access. The site-config fetch starts as `null`
  // and only populates on success  until then we treat the gate as off
  // (fail closed). Used here only for the empty-state CTA; the actual
  // Compose link in the header is owned by `<Layout>`.
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
        const qp = new URLSearchParams();
        qp.set("limit", String(FEED_LIMIT));
        if (nextCursor) qp.set("cursor", nextCursor);
        if (category) qp.set("category", category);
        if (q) qp.set("q", q);
        const response = await fetchApi(`/api/feed?${qp.toString()}`);
        if (!response.ok) {
          throw new Error(t("feed.loadFailed"));
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
    [category, q, t],
  );

  // Re-load from page 0 whenever the active category or search changes.
  useEffect(() => {
    setPosts([]);
    setCursor(null);
    setHasMore(false);
    loadPage(null);
  }, [loadPage]);

  function submitSearch(e) {
    e.preventDefault();
    setParams((prev) => {
      const next = new URLSearchParams(prev);
      const trimmed = searchInput.trim();
      if (trimmed) next.set("q", trimmed);
      else next.delete("q");
      return next;
    });
  }
  function clearSearch() {
    setSearchInput("");
    setParams((prev) => {
      const next = new URLSearchParams(prev);
      next.delete("q");
      return next;
    });
  }

  return (
    <Layout>
      <h1 className="sr-only">{t("feed.heading")}</h1>
      <InviteSplash />

      <form className="feed-search" onSubmit={submitSearch} role="search">
        <input
          type="search"
          className="feed-search-input"
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          placeholder={t("feed.searchPlaceholder")}
          aria-label={t("feed.searchSubmitAria")}
        />
        {q && (
          <button
            type="button"
            className="feed-search-clear"
            onClick={clearSearch}
            aria-label={t("feed.searchClearAria")}
          >
            ×
          </button>
        )}
      </form>

      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}

      {loading && posts.length === 0 && (
        <section
          className="feed-list"
          aria-label={t("feed.loadingAria")}
          aria-busy="true"
        >
          {Array.from({ length: 3 }).map((_, i) => (
            <SkeletonCard key={i} />
          ))}
        </section>
      )}

      {!loading && posts.length === 0 && !error && q && (
        <section className="feed-empty">
          <p>{t("feed.noResultsFor", { query: q })}</p>
          <p className="feed-empty-hint">
            <button
              type="button"
              className="feed-link-button"
              onClick={clearSearch}
            >
              {t("feed.clearSearch")}
            </button>
          </p>
        </section>
      )}

      {!loading && posts.length === 0 && !error && !q && (
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
            {loading ? tCommon("loading") : tCommon("loadMore")}
          </button>
        </div>
      )}
    </Layout>
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
  const { t } = useTranslation("feed");
  if (!user) {
    if (!publicFeedEnabled) {
      return (
        <section className="feed-empty">
          <p>{t("feed.anonInviteOnly")}</p>
          <p className="feed-empty-hint">
            <Trans
              i18nKey="feed:feed.anonInviteOnlyHint"
              components={{ sign: <Link to="/login" /> }}
            />
          </p>
        </section>
      );
    }
    return (
      <section className="feed-empty">
        <p>{t("feed.anonNothingShared")}</p>
        <p className="feed-empty-hint">
          <Trans
            i18nKey="feed:feed.anonNothingSharedHint"
            components={{ sign: <Link to="/login" /> }}
          />
        </p>
      </section>
    );
  }
  if (canCompose) {
    return (
      <section className="feed-empty">
        <p>{t("feed.composeEmpty")}</p>
        <p className="feed-empty-hint">
          <Trans
            i18nKey="feed:feed.composeEmptyHint"
            components={{ compose: <Link to="/compose" /> }}
          />
        </p>
      </section>
    );
  }
  return (
    <section className="feed-empty">
      <p>{t("feed.noPostsYet")}</p>
    </section>
  );
}

import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../../contexts/AuthContext";
import { fetchAuthorFeed, fetchProfile } from "./api";
import { recordPersonVisit } from "../../utils/browseHistory";
import PostCard from "../feed/PostCard";
import ArticleCard from "../articles/ArticleCard";
import { fetchUserArticles, fetchMyDrafts } from "../articles/api";
import SkeletonCard from "../../components/SkeletonCard";
import FollowButton from "../profile/FollowButton";
import "./Profile.css";

const FEED_LIMIT = 20;

/// Public profile page at `/u/:handle`. Anyone who can read the public
/// feed can view a profile; the per-post tier filter still applies to the
/// posts list, so a commenter looking at a poster's profile sees only the
/// posts visible to them.
///
/// The `Edit profile` link only renders when the viewer is the profile's
/// owner. The author-filtered feed is paginated with the same `next_cursor`
/// pattern as `/api/feed`.
export default function Profile() {
  const { handle } = useParams();
  const { user: viewer } = useAuth();
  const [profile, setProfile] = useState(null);
  const [profileLoading, setProfileLoading] = useState(true);
  const [profileError, setProfileError] = useState("");

  const [posts, setPosts] = useState([]);
  const [cursor, setCursor] = useState(null);
  const [hasMore, setHasMore] = useState(false);
  const [postsLoading, setPostsLoading] = useState(false);
  const [postsError, setPostsError] = useState("");

  // Tabbed content view: "posts" | "articles" | "drafts".
  // The "drafts" tab is only rendered when the viewer is the owner.
  const [activeTab, setActiveTab] = useState<"posts" | "articles" | "drafts">(
    "posts",
  );
  const [articles, setArticles] = useState([]);
  const [articlesLoading, setArticlesLoading] = useState(false);
  const [articlesError, setArticlesError] = useState("");

  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");
  const { t: tArticles } = useTranslation("articles");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setProfileLoading(true);
      setProfileError("");
      try {
        const response = await fetchProfile(handle);
        if (response.status === 404 || response.status === 401) {
          if (!cancelled) {
            setProfile(null);
            setProfileError(t("profile.notFound"));
          }
          return;
        }
        if (!response.ok) {
          throw new Error(t("profile.loadFailed"));
        }
        const data = await response.json();
        if (!cancelled) {
          setProfile(data);
          if (!viewer || viewer.id !== data.user_id) {
            recordPersonVisit(data.handle);
          }
        }
      } catch (err) {
        if (!cancelled) setProfileError(err.message);
      } finally {
        if (!cancelled) setProfileLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [handle, viewer, t]);

  const authorId = profile?.user_id;
  const loadPostsPage = useCallback(
    async (nextCursor) => {
      if (!authorId) return;
      setPostsLoading(true);
      setPostsError("");
      try {
        const params = new URLSearchParams();
        params.set("limit", String(FEED_LIMIT));
        params.set("author", authorId);
        params.set("kind", "posts");
        if (nextCursor) params.set("cursor", nextCursor);
        const response = await fetchAuthorFeed(params.toString());
        if (!response.ok) throw new Error(t("profile.postsLoadFailed"));
        const data = await response.json();
        setPosts((prev) =>
          nextCursor ? [...prev, ...data.posts] : data.posts,
        );
        setCursor(data.next_cursor);
        setHasMore(Boolean(data.next_cursor));
      } catch (err) {
        setPostsError(err.message);
      } finally {
        setPostsLoading(false);
      }
    },
    [authorId, t],
  );

  useEffect(() => {
    if (authorId) {
      // Reset the list and cursor synchronously before the fetch so the
      // previous author's posts disappear behind the spinner; the
      // compiler-aware rule flags the pre-await writes.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setPosts([]);
      setCursor(null);
      setHasMore(false);
      loadPostsPage(null);
    }
  }, [authorId, loadPostsPage]);

  const isSelf = viewer && profile && viewer.id === profile.user_id;

  // Lazy-fetch articles when the Articles or Drafts tab is selected.
  // The drafts surface is owner-only (server-side authentication enforces it).
  useEffect(() => {
    if (!authorId) return;
    if (activeTab === "posts") return;
    let cancelled = false;
    async function load() {
      setArticlesLoading(true);
      setArticlesError("");
      try {
        const response =
          activeTab === "drafts"
            ? await fetchMyDrafts()
            : await fetchUserArticles(authorId);
        if (!response.ok) throw new Error(tArticles("browse.loadFailed"));
        const data = await response.json();
        if (!cancelled) setArticles(data.articles);
      } catch (err) {
        if (!cancelled) setArticlesError(err.message);
      } finally {
        if (!cancelled) setArticlesLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [authorId, activeTab, tArticles]);

  return (
    <>
      {profileLoading && <SkeletonCard />}

      {!profileLoading && profileError && (
        <div className="alert alert-error" role="alert">
          {profileError}
        </div>
      )}

      {!profileLoading && profile && (
        <>
          <ProfileCard
            profile={profile}
            viewer={viewer}
            isSelf={isSelf}
            onProfileChange={setProfile}
          />

          <div className="feed-kind-toggle" role="group" aria-label={t("profile.postsHeading")}>
            <button
              type="button"
              aria-pressed={activeTab === "posts"}
              className={`feed-kind-toggle-option${activeTab === "posts" ? " is-active" : ""}`}
              onClick={() => setActiveTab("posts")}
            >
              {tArticles("profile.tabPosts")}
            </button>
            <button
              type="button"
              aria-pressed={activeTab === "articles"}
              className={`feed-kind-toggle-option${activeTab === "articles" ? " is-active" : ""}`}
              onClick={() => setActiveTab("articles")}
            >
              {tArticles("profile.tabArticles")}
            </button>
            {isSelf && (
              <button
                type="button"
                aria-pressed={activeTab === "drafts"}
                className={`feed-kind-toggle-option${activeTab === "drafts" ? " is-active" : ""}`}
                onClick={() => setActiveTab("drafts")}
              >
                {tArticles("profile.tabDrafts")}
              </button>
            )}
          </div>
          <div
            aria-label={tArticles(`profile.tab${activeTab.charAt(0).toUpperCase()}${activeTab.slice(1)}` as never)}
          >

          {activeTab === "posts" && (
            <>
              {postsError && (
                <div className="alert alert-error" role="alert">
                  {postsError}
                </div>
              )}

              {postsLoading && posts.length === 0 && (
                <section
                  className="feed-list"
                  aria-label={t("profile.loadingPostsAria")}
                  aria-busy="true"
                >
                  {Array.from({ length: 2 }).map((_, i) => (
                    <SkeletonCard key={i} />
                  ))}
                </section>
              )}

              {!postsLoading && posts.length === 0 && !postsError && (
                <p className="muted">{t("profile.postsEmpty")}</p>
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
                    disabled={postsLoading}
                    onClick={() => loadPostsPage(cursor)}
                  >
                    {postsLoading ? tCommon("loading") : tCommon("loadMore")}
                  </button>
                </div>
              )}
            </>
          )}

          {(activeTab === "articles" || activeTab === "drafts") && (
            <>
              {articlesError && (
                <div className="alert alert-error" role="alert">
                  {articlesError}
                </div>
              )}
              {articlesLoading && articles.length === 0 && (
                <div aria-busy="true">
                  {Array.from({ length: 2 }).map((_, i) => (
                    <SkeletonCard key={i} />
                  ))}
                </div>
              )}
              {!articlesLoading && articles.length === 0 && !articlesError && (
                <p className="muted">
                  {activeTab === "drafts"
                    ? tArticles("profile.noDrafts")
                    : tArticles("profile.noArticles")}
                </p>
              )}
              <div className="articles-list">
                {articles.map((a) => (
                  <ArticleCard key={a.id} summary={a} />
                ))}
              </div>
            </>
          )}
          </div>
        </>
      )}
    </>
  );
}

function ProfileCard({ profile, viewer, isSelf, onProfileChange }) {
  const { t } = useTranslation("browse");
  const display = profile.display_name || profile.handle;

  const onFollowChange = useCallback(
    ({ you_follow, follows_you }) => {
      onProfileChange((prev) => {
        if (!prev) return prev;
        const wasFollowing = prev.you_follow;
        const willFollow = you_follow;
        const delta = wasFollowing === willFollow ? 0 : willFollow ? 1 : -1;
        return {
          ...prev,
          you_follow,
          follows_you,
          follower_count: Math.max(0, (prev.follower_count ?? 0) + delta),
        };
      });
    },
    [onProfileChange],
  );

  return (
    <article className="profile-card">
      <div className="profile-avatar-large" aria-hidden={!profile.avatar_url}>
        {profile.avatar_url ? (
          <img src={profile.avatar_url} alt={`${display} avatar`} />
        ) : (
          <span>{computeInitials(display)}</span>
        )}
      </div>
      <div className="profile-card-meta">
        <h1 className="profile-display-name">{display}</h1>
        <div className="profile-handle-row">
          <span className="profile-handle">@{profile.handle}</span>
          {profile.pronouns && (
            <span className="profile-pronouns">· {profile.pronouns}</span>
          )}
          {!isSelf && profile.follows_you && (
            <span className="profile-pill">{t("profile.followsYou")}</span>
          )}
        </div>
        <div className="profile-counts">
          <span className="profile-count">
            {t("profile.followerCount", { count: profile.follower_count ?? 0 })}
          </span>
          <span className="profile-count">
            {t("profile.followingCount", {
              count: profile.following_count ?? 0,
            })}
          </span>
        </div>
        {profile.bio && <p className="profile-bio">{profile.bio}</p>}
        <div className="profile-card-actions">
          {isSelf && (
            <Link to="/settings/profile" className="btn-secondary">
              {t("profile.edit")}
            </Link>
          )}
          {!isSelf && viewer && (
            <FollowButton
              userId={profile.user_id}
              youFollow={!!profile.you_follow}
              followsYou={!!profile.follows_you}
              onChange={onFollowChange}
            />
          )}
          {!isSelf && !viewer && (
            <Link to="/login" className="btn-secondary">
              {t("profile.signInToFollow")}
            </Link>
          )}
        </div>
      </div>
    </article>
  );
}

function computeInitials(name) {
  if (!name) return "??";
  if (name.includes("@")) return name.slice(0, 2).toUpperCase();
  const parts = name.trim().split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]).join("").toUpperCase() || "??";
}

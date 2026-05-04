import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import { recordPersonVisit } from "../utils/browseHistory";
import Layout from "../components/Layout";
import PostCard from "../components/PostCard";
import SkeletonCard from "../components/SkeletonCard";
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
  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setProfileLoading(true);
      setProfileError("");
      try {
        const response = await fetchApi(
          `/api/profiles/${encodeURIComponent(handle)}`,
        );
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
  }, [handle, viewer]);

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
        if (nextCursor) params.set("cursor", nextCursor);
        const response = await fetchApi(`/api/feed?${params.toString()}`);
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
    [authorId],
  );

  useEffect(() => {
    if (authorId) {
      setPosts([]);
      setCursor(null);
      setHasMore(false);
      loadPostsPage(null);
    }
  }, [authorId, loadPostsPage]);

  const isSelf = viewer && profile && viewer.id === profile.user_id;

  return (
    <Layout>
      {profileLoading && <SkeletonCard />}

      {!profileLoading && profileError && (
        <div className="alert alert-error">{profileError}</div>
      )}

      {!profileLoading && profile && (
        <>
          <ProfileCard profile={profile} isSelf={isSelf} />

          <h2 className="profile-posts-heading">{t("profile.postsHeading")}</h2>

          {postsError && (
            <div className="alert alert-error">{postsError}</div>
          )}

          {postsLoading && posts.length === 0 && (
            <section
              className="feed-list"
              aria-label={t("profile.loadingPostsAria")}
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
    </Layout>
  );
}

function ProfileCard({ profile, isSelf }) {
  const { t } = useTranslation("browse");
  const display = profile.display_name || profile.handle;
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
        </div>
        {profile.bio && <p className="profile-bio">{profile.bio}</p>}
        {isSelf && (
          <div className="profile-card-actions">
            <Link to="/settings/profile" className="btn-secondary">
              {t("profile.edit")}
            </Link>
          </div>
        )}
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

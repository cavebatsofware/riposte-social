import { useEffect, useState } from "react";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";

/// Like-button + count for one post, used in both feed cards and the
/// permalink view. Authenticated callers click to toggle; anonymous callers
/// see the count rendered as static text.
///
/// Optimistic update: the local count and viewer-reaction state flip
/// immediately on click and only roll back if the request fails. The server
/// response replaces the local state on success so the bar stays in sync
/// even with concurrent reactions from other clients.
///
/// `post` shape (relevant fields): `reaction_counts: { like: N }`,
/// `viewer_reaction_kinds: ["like"]`.
///
/// In the feed variant the parent <PostCard> wraps the whole article in a
/// react-router <Link>; the click handler calls `e.preventDefault()` +
/// `stopPropagation()` so toggling a reaction doesn't navigate to the
/// permalink.
export default function ReactionBar({ post }) {
  const { user } = useAuth();
  const [counts, setCounts] = useState(post.reaction_counts || {});
  const [viewerKinds, setViewerKinds] = useState(post.viewer_reaction_kinds || []);
  const [pending, setPending] = useState(false);

  // Reset local state when the parent passes a different post (rare, but
  // happens if the feed remounts the same component slot with another row).
  useEffect(() => {
    setCounts(post.reaction_counts || {});
    setViewerKinds(post.viewer_reaction_kinds || []);
  }, [post.id, post.reaction_counts, post.viewer_reaction_kinds]);

  const liked = viewerKinds.includes("like");
  const likeCount = counts["like"] || 0;
  const canReact = Boolean(user);

  async function toggleLike(e) {
    e.preventDefault();
    e.stopPropagation();
    if (!canReact || pending) return;

    const wasLiked = liked;
    const optimisticCounts = { ...counts };
    optimisticCounts.like = (optimisticCounts.like || 0) + (wasLiked ? -1 : 1);
    if (optimisticCounts.like <= 0) delete optimisticCounts.like;
    const optimisticKinds = wasLiked
      ? viewerKinds.filter((k) => k !== "like")
      : [...viewerKinds, "like"];

    setPending(true);
    setCounts(optimisticCounts);
    setViewerKinds(optimisticKinds);

    try {
      const response = wasLiked
        ? await fetchApi(`/api/posts/${post.id}/reactions/like`, {
            method: "DELETE",
          })
        : await fetchApi(`/api/posts/${post.id}/reactions`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ kind: "like" }),
          });
      if (!response.ok) throw new Error("reaction request failed");
      const data = await response.json();
      // Server response is authoritative; replaces optimistic state.
      setCounts(data.reaction_counts || {});
      setViewerKinds(data.viewer_reaction_kinds || []);
    } catch (err) {
      // Roll back optimistic update.
      setCounts(post.reaction_counts || {});
      setViewerKinds(post.viewer_reaction_kinds || []);
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="reaction-bar">
      {canReact ? (
        <button
          type="button"
          className={`reaction-btn ${liked ? "liked" : ""}`}
          onClick={toggleLike}
          disabled={pending}
          aria-pressed={liked}
          aria-label={liked ? "Unlike this post" : "Like this post"}
        >
          <span className="reaction-icon" aria-hidden="true">
            {liked ? "♥" : "♡"}
          </span>
          <span className="reaction-label">{liked ? "Liked" : "Like"}</span>
          {likeCount > 0 && (
            <span className="reaction-count">{likeCount}</span>
          )}
        </button>
      ) : (
        <span
          className="reaction-static"
          aria-label={`${likeCount} ${likeCount === 1 ? "like" : "likes"}`}
        >
          <span className="reaction-icon" aria-hidden="true">
            ♥
          </span>
          {likeCount > 0 ? likeCount : "0"}
        </span>
      )}
    </div>
  );
}

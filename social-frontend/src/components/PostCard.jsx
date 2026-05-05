import { useLayoutEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import DOMPurify from "dompurify";
import { useAuth } from "../contexts/AuthContext";
import { formatRelativeTime } from "../utils/formatTime";
import MediaLightbox from "./MediaLightbox";
import ReactionBar from "./ReactionBar";
import VisibilityBadge from "./VisibilityBadge";
import VisibilityMenu from "./VisibilityMenu";

/// Renders one post in the feed or on its permalink.
///
/// `post` is the shape returned by `GET /api/feed` and `GET /api/posts/{id}`:
/// `{ id, author_id, author_display, body, body_html, visibility,
///    published_at, media: [{ id, url, mime_type, ... }],
///    reaction_counts: { like: 5, ... }, viewer_reaction_kinds: [],
///    comment_count: 3 }`.
///
/// `body_html` is sanitized server-side via `ammonia` in
/// `src/posts/markdown.rs` before it reaches the client. We run DOMPurify
/// on the same string before injecting via `dangerouslySetInnerHTML` as
/// defense-in-depth. If the server is ever compromised or the ammonia
/// allowlist drifts, the client-side pass still strips script tags,
/// event-handler attributes, and `javascript:` URLs.
///
/// `variant`:
/// - `"feed"` (default): wraps the card in a `<Link>` to the permalink so
///   the entire card is clickable; multi-image media renders as a grid.
/// - `"permalink"`: no wrapping link; the first media attachment renders
///   as a full-width hero, subsequent attachments as a thumbnail row.
export default function PostCard({ post, variant = "feed" }) {
  const { user } = useAuth();
  const { t, i18n } = useTranslation("feed");
  const author =
    post.author_display || post.author_handle || t("fallbackAuthor");
  const initials = computeInitials(author);
  const time = formatRelativeTime(post.published_at, i18n.language);
  const safeHtml = DOMPurify.sanitize(post.body_html || "");
  // Stop the click on the byline from bubbling up to the card-wrapping
  // <Link> on feed variant — clicking the avatar should land on the
  // profile, not the post permalink.
  const stopBubble = (e) => e.stopPropagation();

  // Lightbox state. Mirrors the Album page pattern (Phase 9d) so post
  // media gets the same click-in carousel — full-screen image/video
  // viewer with prev/next, swipe, ESC. Null = closed; an index opens
  // at that media position.
  const [lightboxIndex, setLightboxIndex] = useState(null);
  function openLightbox(idx, e) {
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }
    setLightboxIndex(idx);
  }

  // Quick visibility toggle (Phase 9b) is shown only when the viewer is
  // the post's author or an administrator. Non-owners get the read-only
  // badge they already had.
  const canEditVisibility =
    user && (user.id === post.author_id || user.role === "administrator");

  const card = (
    <article className="post-card">
      <h2 className="sr-only">
        {t("postCard.articleHeading", { author, time })}
      </h2>
      <header className="post-meta">
        <PostAvatar
          handle={post.author_handle}
          avatarUrl={post.author_avatar_url}
          initials={initials}
          author={author}
          stopBubble={stopBubble}
        />
        <div>
          <PostAuthorName
            handle={post.author_handle}
            author={author}
            stopBubble={stopBubble}
          />
          <div className="post-meta-line">
            <span>{time}</span>
            <span className="post-meta-dot" aria-hidden="true" />
            {canEditVisibility && !post.category ? (
              <VisibilityMenu post={post} />
            ) : (
              <VisibilityBadge
                visibility={post.effective_visibility || post.visibility}
                fromCategory={Boolean(post.category)}
              />
            )}
            {post.category && (
              <>
                <span className="post-meta-dot" aria-hidden="true" />
                <Link
                  to={`/?category=${encodeURIComponent(post.category.slug)}`}
                  className="post-category-chip"
                  style={
                    post.category.color
                      ? { backgroundColor: post.category.color }
                      : undefined
                  }
                  onClick={stopBubble}
                >
                  {post.category.name}
                </Link>
              </>
            )}
          </div>
        </div>
      </header>

      <PostBody safeHtml={safeHtml} variant={variant} />

      {post.media && post.media.length > 0 && (
        <PostMedia media={post.media} onOpen={openLightbox} />
      )}

      <PostActions post={post} variant={variant} target={{ kind: "post", postId: post.id }} />

      {variant === "feed" &&
        post.top_comments &&
        post.top_comments.length > 0 && (
          <TopComments
            comments={post.top_comments}
            commentCount={post.comment_count || 0}
            postId={post.id}
          />
        )}
    </article>
  );

  // Lightbox renders as a portal-like overlay outside the card-link
  // wrapper so a) it covers the full viewport regardless of where the
  // PostCard sits in the layout, and b) clicking inside the lightbox
  // doesn't bubble up to the card's <Link>.
  const lightbox = lightboxIndex !== null && post.media && (
    <MediaLightbox
      items={post.media}
      index={lightboxIndex}
      onIndex={setLightboxIndex}
      onClose={() => setLightboxIndex(null)}
    />
  );

  if (variant === "feed") {
    return (
      <>
        <Link to={`/post/${post.id}`} className="post-card-link">
          {card}
        </Link>
        {lightbox}
      </>
    );
  }
  return (
    <>
      {card}
      {lightbox}
    </>
  );
}

/// Avatar bubble in the post-meta header. Renders an image when the
/// author has uploaded one, otherwise initials. Wrapped in a `<Link>` to
/// `/u/{handle}` when the handle is known so the entire bubble is
/// clickable; falls back to a plain div otherwise.
function PostAvatar({ handle, avatarUrl, initials, author, stopBubble }) {
  const inner = avatarUrl ? (
    <img src={avatarUrl} alt={`${author} avatar`} />
  ) : (
    <span aria-hidden="true">{initials}</span>
  );
  if (handle) {
    return (
      <Link
        to={`/u/${handle}`}
        className="post-avatar post-avatar-link"
        onClick={stopBubble}
        aria-label={`${author} profile`}
      >
        {inner}
      </Link>
    );
  }
  return (
    <div className="post-avatar" aria-hidden="true">
      {inner}
    </div>
  );
}

/// Author display name, linked to `/u/{handle}` when available.
function PostAuthorName({ handle, author, stopBubble }) {
  if (handle) {
    return (
      <Link
        to={`/u/${handle}`}
        className="post-author post-author-link"
        onClick={stopBubble}
      >
        {author}
      </Link>
    );
  }
  return <div className="post-author">{author}</div>;
}

/// Renders the post body. On the feed variant, clamp height and add the
/// "Read more" gradient overlay only when the rendered body actually
/// overflows the clamp — short posts get neither the clamp nor the fade.
/// Detection runs on layout (post-paint, pre-flicker) and re-runs on the
/// body content changing.
function PostBody({ safeHtml, variant }) {
  const ref = useRef(null);
  const [clamped, setClamped] = useState(false);

  useLayoutEffect(() => {
    if (variant !== "feed") {
      setClamped(false);
      return;
    }
    const el = ref.current;
    if (!el) return;
    // scrollHeight is the full content height; if it exceeds clientHeight
    // the post overflows the CSS max-height and the fade should render.
    setClamped(el.scrollHeight > el.clientHeight + 1);
  }, [safeHtml, variant]);

  return (
    <div
      ref={ref}
      className="post-body"
      data-clamped={clamped ? "true" : "false"}
      dangerouslySetInnerHTML={{ __html: safeHtml }}
    />
  );
}

/// Renders one media item — image or video — wrapped in a click-target
/// button that opens the lightbox at the given index. Mirrors the
/// Album page (Phase 9d): videos render as a thumbnail with a play
/// badge rather than an inline `<video controls>`, since the lightbox
/// provides controls + autoplay when the user clicks in. Keeps the
/// behavior consistent across post and album surfaces.
function MediaItem({ m, index, className, onOpen }) {
  const { t } = useTranslation("feed");
  const inner =
    m.media_kind === "video" ? (
      <>
        <video
          className={className}
          src={m.url}
          muted
          playsInline
          preload="metadata"
        >
          {t("postCard.videoFallback")}
        </video>
        <span className="post-media-video-badge" aria-hidden="true">
          ▶
        </span>
      </>
    ) : (
      <img className={className} src={m.url} alt={m.caption || ""} />
    );
  return (
    <button
      type="button"
      className="post-media-item"
      onClick={(e) => onOpen(index, e)}
      aria-label={m.caption || t("postCard.openMediaAria", { index: index + 1 })}
    >
      {inner}
    </button>
  );
}

/// Unified post-media layout. Renders a single full-width item or a
/// 2-column grid of items, the same way on both feed and permalink
/// variants. The previous hero+row split made 2-image posts look wrong
/// on permalink (one thumb half-width, one empty grid cell). Unifying
/// keeps the visual rhythm consistent across both surfaces.
function PostMedia({ media, onOpen }) {
  if (media.length === 1) {
    const [m] = media;
    return (
      <div className="post-media-single">
        <MediaItem m={m} index={0} onOpen={onOpen} />
      </div>
    );
  }
  return (
    <div className="post-media-row">
      {media.map((m, i) => (
        <MediaItem key={m.id} m={m} index={i} onOpen={onOpen} />
      ))}
    </div>
  );
}

/// Action footer: interactive `<ReactionBar />` plus a comment-count
/// indicator. On the feed variant the wrapping `<Link>` makes the comment
/// count area implicitly clickable; on the permalink variant the count is
/// suppressed because the comment thread renders right below.
function PostActions({ post, variant, target }) {
  const { t } = useTranslation("feed");
  const commentCount = post.comment_count || 0;
  const showCommentCount = variant !== "permalink";
  return (
    <footer className="post-actions" aria-label={t("postCard.actionsAria")}>
      <ReactionBar target={target} state={post} />
      {showCommentCount && (
        <span className="post-actions-comments">
          {t("postCard.commentCount", { count: commentCount })}
        </span>
      )}
    </footer>
  );
}

/// Inline preview of the latest 1-3 comments for a feed PostCard. The
/// backend returns top_comments newest-first; we reverse so the row reads
/// chronologically top-to-bottom, with the latest comment last (matching
/// the way the full thread reads on the permalink page). Bodies are
/// truncated to a single line at ~140 chars; the wrapping card-link
/// handles navigation to the full thread.
function TopComments({ comments, commentCount, postId }) {
  const { t } = useTranslation("feed");
  const ordered = [...comments].reverse();
  const remaining = commentCount - comments.length;
  return (
    <div
      className="post-top-comments"
      aria-label={t("postCard.recentCommentsAria")}
    >
      {ordered.map((c) => (
        <TopCommentRow key={c.id} comment={c} />
      ))}
      {remaining > 0 && (
        <Link
          to={`/post/${postId}#comments`}
          className="post-top-comments-more"
          onClick={(e) => e.stopPropagation()}
        >
          {t("postCard.viewMore", { count: remaining })}
        </Link>
      )}
    </div>
  );
}

function TopCommentRow({ comment }) {
  const { t } = useTranslation("feed");
  const author =
    comment.author_display || comment.author_handle || t("fallbackAuthor");
  // The inline snippet is one line of prose, not rendered markdown — strip
  // tags from the server-sanitized body_html (preferred) or fall back to
  // the raw markdown source. Rendering "## Heading" verbatim looks broken
  // in the dense feed row.
  const plain = htmlToPlainText(comment.body_html) || comment.body || "";
  const body = truncate(plain, 140);
  return (
    <div className="post-top-comment">
      <span className="post-top-comment-author">{author}</span>
      <span className="post-top-comment-body">{body}</span>
    </div>
  );
}

/// Turn server-sanitized HTML into a single plain-text string. Sanitizes
/// again with DOMPurify (defense-in-depth), parses, and reads `textContent`.
/// Block-level newlines collapse to single spaces so the snippet stays
/// on one line.
function htmlToPlainText(html) {
  if (!html) return "";
  const safe = DOMPurify.sanitize(html);
  if (typeof window === "undefined" || !window.DOMParser) return safe;
  const doc = new window.DOMParser().parseFromString(safe, "text/html");
  const text = (doc.body && doc.body.textContent) || "";
  return text.replace(/\s+/g, " ").trim();
}

function truncate(s, max) {
  if (s.length <= max) return s;
  // Trim to the next word boundary up to max so we don't snap mid-word.
  const slice = s.slice(0, max);
  const lastSpace = slice.lastIndexOf(" ");
  const cut = lastSpace > max * 0.6 ? lastSpace : max;
  return s.slice(0, cut).replace(/\s+$/, "") + "…";
}

function computeInitials(name) {
  if (!name) return "??";
  if (name.includes("@")) {
    return name.slice(0, 2).toUpperCase();
  }
  const parts = name.trim().split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]).join("").toUpperCase() || "??";
}


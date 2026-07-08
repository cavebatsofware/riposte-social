import React, { useLayoutEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import DOMPurify from "dompurify";
import { useAuth } from "../../contexts/AuthContext";
import { formatRelativeTime } from "../../utils/formatTime";
import { useMediaVariants } from "../../hooks/useMediaVariants";
import MediaLightbox from "../engagement/MediaLightbox";
import ReactionBar from "../engagement/ReactionBar";
import VisibilityBadge from "../../components/VisibilityBadge";
import VisibilityMenu from "../../components/VisibilityMenu";
import ShareMenu from "../../components/ShareMenu";
import type { PostMediaResponse } from "../../types/api";

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
/// - `"feed"` (default): renders an "Open post" link in the meta line so
///   the permalink renders in the post header.
/// - `"permalink"`: no permalink rendered.
///   
///  The first media attachment
///  renders as a full-width hero, subsequent attachments as a thumbnail
///  row.
export default function PostCard({ post, variant = "feed" }) {
  const { user } = useAuth();
  const { t, i18n } = useTranslation("feed");
  const author =
    post.author_display || post.author_handle || t("fallbackAuthor");
  const initials = computeInitials(author);
  const time = formatRelativeTime(post.published_at, i18n.language);
  const safeHtml = DOMPurify.sanitize(post.body_html || "");

  const [lightboxIndex, setLightboxIndex] = useState(null);
  function openLightbox(idx, e) {
    if (e) e.preventDefault();
    setLightboxIndex(idx);
  }

  // Quick visibility toggle is shown only when the viewer is
  // the post's author or an administrator. Non-owners get the read-only
  // badge they already had.
  const canEditVisibility =
    user && (user.id === post.author_id || user.role === "administrator");

  const card = (
    <article className="post-card">
      <h2 className="sr-only">
        {t("postCard.articleAria", { author, time })}
      </h2>
      <header className="post-meta">
        <PostAvatar
          handle={post.author_handle}
          avatarUrl={post.author_avatar_url}
          initials={initials}
          author={author}
        />
        <div className="post-meta-main">
          <PostAuthorName
            handle={post.author_handle}
            author={author}
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
                >
                  {post.category.name}
                </Link>
              </>
            )}
          </div>
        </div>
        {variant === "feed" && (
          <Link to={`/post/${post.id}`} className="post-meta-open">
            {t("postCard.openPost")}
          </Link>
        )}
      </header>

      <PostBody safeHtml={safeHtml} variant={variant} />

      {post.media && post.media.length > 0 && (
        <PostMedia parentId={post.id} media={post.media} onOpen={openLightbox} />
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

  const lightbox = lightboxIndex !== null && post.media && (
    <MediaLightbox
      items={post.media}
      index={lightboxIndex}
      postId={post.id}
      onIndex={setLightboxIndex}
      onClose={() => setLightboxIndex(null)}
    />
  );

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
function PostAvatar({ handle, avatarUrl, initials, author }) {
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
function PostAuthorName({ handle, author }) {
  if (handle) {
    return (
      <Link
        to={`/u/${handle}`}
        className="post-author post-author-link"
      >
        {author}
      </Link>
    );
  }
  return <div className="post-author">{author}</div>;
}

/// Renders the post body. On the feed variant, clamp height and add the
/// "Read more" gradient overlay only when the rendered body actually
/// overflows the clamp  short posts get neither the clamp nor the fade.
/// Detection runs on layout (post-paint, pre-flicker) and re-runs on the
/// body content changing.
function PostBody({ safeHtml, variant }) {
  const ref = useRef(null);
  const [clamped, setClamped] = useState(false);

  useLayoutEffect(() => {
    // Measuring layout requires a real DOM, so this stays a layout
    // effect; the compiler-aware rule still flags setState in effect.
    if (variant !== "feed") {
      // eslint-disable-next-line react-hooks/set-state-in-effect
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

/// Renders one media item  image or video  wrapped in a click-target
/// button that opens the lightbox at the given index. Mirrors the
/// Album page: videos render as a thumbnail with a play
/// badge rather than an inline `<video controls>`, since the lightbox
/// provides controls + autoplay when the user clicks in. Keeps the
/// behavior consistent across post and album surfaces.
function MediaItem({
  m,
  index,
  className = "",
  onOpen,
  thumbnail,
}: {
  m: PostMediaResponse;
  index: number;
  className?: string;
  onOpen: (i: number, e: React.MouseEvent) => void;
  thumbnail: string | null | undefined;
}) {
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
          <track default kind="captions" srcLang="en" src="data:text/vtt;base64,V0VCVlRUCgo=" />
          <track kind="descriptions" srcLang="en" src="data:text/vtt;base64,V0VCVlRUCgo=" />
          {t("postCard.videoFallback")}
        </video>
        <span className="post-media-video-badge" aria-hidden="true">
          ▶
        </span>
      </>
    ) : thumbnail ? (
      <img className={className} src={thumbnail} alt={m.caption || ""} />
    ) : (
      <span
        className={`post-media-placeholder ${className}`}
        aria-hidden="true"
        style={
          m.width && m.height
            ? { aspectRatio: `${m.width} / ${m.height}` }
            : undefined
        }
      />
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
function PostMedia({ parentId, media, onOpen }) {
  const imageIds = useMemo(
    () => media.filter((m) => m.media_kind !== "video").map((m) => m.id),
    [media],
  );
  const variants = useMediaVariants(parentId, imageIds);
  if (media.length === 1) {
    const [m] = media;
    return (
      <div className="post-media-single">
        <MediaItem
          m={m}
          index={0}
          onOpen={onOpen}
          thumbnail={variants[m.id]}
        />
      </div>
    );
  }
  return (
    <div className="post-media-row">
      {media.map((m, i) => (
        <MediaItem
          key={m.id}
          m={m}
          index={i}
          onOpen={onOpen}
          thumbnail={variants[m.id]}
        />
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
  const isPublic =
    (post.effective_visibility || post.visibility) === "public";
  return (
    <footer className="post-actions" aria-label={t("postCard.actionsAria")}>
      <ReactionBar target={target} state={post} />
      <div className="post-actions-end">
        {showCommentCount && (
          <span className="post-actions-comments">
            {t("postCard.commentCount", { count: commentCount })}
          </span>
        )}
        <ShareMenu
          path={`/post/${post.id}`}
          title={post.slug || undefined}
          isPublic={isPublic}
        />
      </div>
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
        <Link to={`/post/${postId}#comments`} className="post-top-comments-more">
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
  // The inline snippet is one line of prose, not rendered markdown  strip
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


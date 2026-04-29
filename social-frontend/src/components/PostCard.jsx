import { Link } from "react-router-dom";
import DOMPurify from "dompurify";

/// Renders one post in the feed or on its permalink.
///
/// `post` is the shape returned by `GET /api/feed` and `GET /api/posts/{id}`:
/// `{ id, author_id, author_display, author_email, body, body_html,
///    visibility, published_at, media: [{ id, url, mime_type, ... }], ... }`.
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
  const author = post.author_display || post.author_email || "Unknown";
  const initials = computeInitials(author);
  const time = formatRelativeTime(post.published_at);
  const safeHtml = DOMPurify.sanitize(post.body_html || "");

  const card = (
    <article className="post-card">
      <header className="post-meta">
        <div className="post-avatar" aria-hidden="true">
          {initials}
        </div>
        <div>
          <div className="post-author">{author}</div>
          <div className="post-meta-line">
            <span>{time}</span>
            <span className="post-meta-dot" aria-hidden="true" />
            <VisibilityBadge visibility={post.visibility} />
          </div>
        </div>
      </header>

      <div
        className="post-body"
        dangerouslySetInnerHTML={{ __html: safeHtml }}
      />

      {post.media && post.media.length > 0 && (
        <PostMedia media={post.media} variant={variant} />
      )}
    </article>
  );

  if (variant === "feed") {
    return (
      <Link to={`/post/${post.id}`} className="post-card-link">
        {card}
      </Link>
    );
  }
  return card;
}

function PostMedia({ media, variant }) {
  if (variant === "permalink" && media.length > 0) {
    const [hero, ...rest] = media;
    return (
      <div className="post-media-permalink">
        <img className="post-media-hero" src={hero.url} alt={hero.caption || ""} />
        {rest.length > 0 && (
          <div className="post-media-row">
            {rest.map((m) => (
              <img key={m.id} src={m.url} alt={m.caption || ""} />
            ))}
          </div>
        )}
      </div>
    );
  }

  if (media.length === 1) {
    const [m] = media;
    return (
      <div className="post-media-single">
        <img src={m.url} alt={m.caption || ""} />
      </div>
    );
  }

  return (
    <div className="post-media-row">
      {media.map((m) => (
        <img key={m.id} src={m.url} alt={m.caption || ""} />
      ))}
    </div>
  );
}

function VisibilityBadge({ visibility }) {
  const cls =
    visibility === "commenters"
      ? "visibility-badge commenters"
      : visibility === "posters"
        ? "visibility-badge posters"
        : "visibility-badge";
  const label =
    visibility === "commenters"
      ? "Commenters"
      : visibility === "posters"
        ? "Posters"
        : "Public";
  return <span className={cls}>{label}</span>;
}

function computeInitials(name) {
  if (!name) return "??";
  if (name.includes("@")) {
    return name.slice(0, 2).toUpperCase();
  }
  const parts = name.trim().split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]).join("").toUpperCase() || "??";
}

function formatRelativeTime(iso) {
  const d = new Date(iso);
  const now = new Date();
  const diffSec = Math.floor((now - d) / 1000);
  if (diffSec < 60) return "just now";
  if (diffSec < 3600) {
    const m = Math.floor(diffSec / 60);
    return `${m} minute${m === 1 ? "" : "s"} ago`;
  }
  if (diffSec < 86400) {
    const h = Math.floor(diffSec / 3600);
    return `${h} hour${h === 1 ? "" : "s"} ago`;
  }
  if (diffSec < 7 * 86400) {
    const days = Math.floor(diffSec / 86400);
    if (days === 1) return "yesterday";
    return `${days} days ago`;
  }
  return d.toLocaleDateString();
}

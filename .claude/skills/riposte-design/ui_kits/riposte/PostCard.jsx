/* global React, ReactionBar */

/// Read-only visibility badge. Tier ∈ {public, commenters, posters, private}.
function VisibilityBadge({ visibility }) {
  const labels = { public: "Public", commenters: "Commenters", posters: "Posters", private: "Private" };
  return <span className={`visibility-badge ${visibility}`}>{labels[visibility] || "Public"}</span>;
}

/// Feed post card. `variant`:
///   "feed"      — body clamped, "Open post" link, top comments preview
///   "permalink" — full body, no top-comments roll-up
function PostCard({ post, variant = "feed", onOpen, onOpenMedia }) {
  const handleOpen = (e) => {
    e.preventDefault();
    onOpen && onOpen(post);
  };

  return (
    <article className="post-card">
      <header className="post-meta">
        <div className="post-avatar" aria-hidden="true">{post.initials}</div>
        <div className="post-meta-main">
          <div className="post-author">{post.author}</div>
          <div className="post-meta-line">
            <span>{post.time}</span>
            <span className="post-meta-dot" aria-hidden="true" />
            <VisibilityBadge visibility={post.visibility} />
            {post.category && (
              <>
                <span className="post-meta-dot" aria-hidden="true" />
                <a className="post-category-chip" href="#">{post.category}</a>
              </>
            )}
          </div>
        </div>
        {variant === "feed" && (
          <button className="post-meta-open" onClick={handleOpen}>Open post</button>
        )}
      </header>

      <div className="post-body" dangerouslySetInnerHTML={{ __html: post.bodyHtml }} />

      {post.media && post.media.length > 0 && (
        <PostMedia media={post.media} onOpenMedia={(i) => onOpenMedia && onOpenMedia(post, i)} />
      )}

      <footer className="post-actions">
        <ReactionBar counts={post.reactionCounts || {}} viewerKinds={post.viewerKinds || []} />
        {variant !== "permalink" && (
          <span>{post.commentCount === 1 ? "1 comment" : `${post.commentCount || 0} comments`}</span>
        )}
      </footer>

      {variant === "feed" && post.topComments && post.topComments.length > 0 && (
        <div className="post-top-comments" aria-label="Recent comments">
          {post.topComments.map((c, i) => (
            <div key={i} className="post-top-comment">
              <span className="post-top-comment-author">{c.author}</span>
              <span className="post-top-comment-body">{c.body}</span>
            </div>
          ))}
        </div>
      )}
    </article>
  );
}

function PostMedia({ media, onOpenMedia }) {
  if (media.length === 1) {
    const [m] = media;
    return (
      <div className="post-media-single" onClick={() => onOpenMedia(0)}>
        {m.placeholder ? (
          <div className="post-media-placeholder" style={{ aspectRatio: "16/9", borderRadius: "var(--radius-md)" }}>
            {m.label}
          </div>
        ) : (
          <img src={m.url} alt={m.caption || ""} />
        )}
      </div>
    );
  }
  return (
    <div className="post-media-row">
      {media.map((m, i) => (
        m.placeholder ? (
          <div key={i} className="post-media-placeholder" style={{ aspectRatio: "1", borderRadius: "var(--radius-md)", cursor: "pointer" }} onClick={() => onOpenMedia(i)}>
            {m.label}
          </div>
        ) : (
          <img key={i} src={m.url} alt={m.caption || ""} onClick={() => onOpenMedia(i)} />
        )
      ))}
    </div>
  );
}

Object.assign(window, { PostCard, VisibilityBadge });

import "./SkeletonCard.css";

/// Placeholder card shown while a feed page or permalink is loading.
///
/// Mimics the structure of `<PostCard>`: avatar circle, two text lines,
/// an optional image-shaped block. Animated via a CSS shimmer keyframe
/// that respects `prefers-reduced-motion`.
export default function SkeletonCard({ withMedia = true }) {
  return (
    <article className="skeleton-card" aria-hidden="true">
      <header className="skeleton-meta">
        <span className="skeleton-avatar" />
        <span className="skeleton-lines">
          <span className="skeleton-line skeleton-line-md" />
          <span className="skeleton-line skeleton-line-sm" />
        </span>
      </header>
      <div className="skeleton-body">
        <span className="skeleton-line skeleton-line-full" />
        <span className="skeleton-line skeleton-line-full" />
        <span className="skeleton-line skeleton-line-3q" />
      </div>
      {withMedia && <div className="skeleton-media" />}
    </article>
  );
}

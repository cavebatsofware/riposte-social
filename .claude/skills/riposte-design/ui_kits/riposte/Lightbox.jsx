/* global React, ReactionBar */
const { useState: useStateLB, useEffect: useEffectLB } = React;

/// Full-bleed media lightbox. The image fills the viewport; a small
/// "engagement panel" peeks ~24px from the bottom as a scrollable
/// affordance for reactions and comments. The peek is the only
/// signal that there's more — there is no separate scrollbar or
/// "View larger" affordance, mirroring the production product.
function Lightbox({ items, index, onIndex, onClose }) {
  const total = items.length;
  const item = items[index];

  useEffectLB(() => {
    function onKey(e) {
      if (e.key === "Escape") onClose();
      if (e.key === "ArrowLeft" && index > 0) onIndex(index - 1);
      if (e.key === "ArrowRight" && index < total - 1) onIndex(index + 1);
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [index, total, onIndex, onClose]);

  return (
    <div className="lightbox-overlay" role="dialog" aria-modal="true" aria-label="Media viewer">
      <button className="lightbox-close" onClick={onClose} aria-label="Close viewer">×</button>
      {index > 0 && (
        <button className="lightbox-nav prev" onClick={() => onIndex(index - 1)} aria-label="Previous">‹</button>
      )}
      {index < total - 1 && (
        <button className="lightbox-nav next" onClick={() => onIndex(index + 1)} aria-label="Next">›</button>
      )}
      <div className="lightbox-counter">{index + 1} / {total}</div>

      <div className="lightbox-scroll">
        <div className="lightbox-image-frame">
          {item.placeholder ? (
            <div style={{ width: "min(720px, 80vw)", aspectRatio: "1", background: "#2a2a2a", color: "#888", display: "flex", alignItems: "center", justifyContent: "center", borderRadius: "8px" }}>
              {item.label}
            </div>
          ) : (
            <img src={item.url} alt={item.caption || ""} />
          )}
        </div>

        <div className="lightbox-engagement" aria-label="Reactions and comments on this item">
          <ReactionBar counts={item.reactionCounts || {}} viewerKinds={item.viewerKinds || []} />
          <h3>Comments</h3>
          <div className="lightbox-engagement-empty">No comments yet.</div>
          <div className="lightbox-engagement-compose">
            <label>Add a comment</label>
            <textarea placeholder="Markdown is supported." />
            <div><button className="btn-primary">Post comment</button></div>
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { Lightbox });

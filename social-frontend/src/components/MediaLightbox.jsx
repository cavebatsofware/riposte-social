import { useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import useFocusTrap from "../utils/useFocusTrap";
import "./MediaLightbox.css";

/// Full-screen lightbox / carousel for album media. Renders a single image
/// or video at a time with prev/next nav, ESC-to-close, swipe on touch,
/// and a small caption strip below the media.
///
/// `items`: array of `{ id, url, mime_type, media_kind, caption }` shaped
///   like an `AlbumMediaResponse`. Order matters — the lightbox steps
///   through them in sequence.
/// `index`: the active item index. Owned by the parent so the call site
///   controls which item is shown.
/// `onClose`, `onIndex`: parent-supplied callbacks.
export default function MediaLightbox({ items, index, onClose, onIndex }) {
  const overlayRef = useFocusTrap(true, { onEscape: onClose });
  const touchStartRef = useRef(null);
  const { t } = useTranslation("browse");

  const total = items.length;
  const item = items[index];

  const goPrev = useCallback(() => {
    if (total === 0) return;
    onIndex((index - 1 + total) % total);
  }, [index, total, onIndex]);

  const goNext = useCallback(() => {
    if (total === 0) return;
    onIndex((index + 1) % total);
  }, [index, total, onIndex]);

  // ArrowLeft / ArrowRight cycle the carousel. Escape and focus
  // restoration are handled by useFocusTrap; body scroll lock is the
  // remaining concern here.
  useEffect(() => {
    function onKey(e) {
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        goPrev();
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        goNext();
      }
    }
    document.addEventListener("keydown", onKey);
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", onKey);
      document.body.style.overflow = "";
    };
  }, [goPrev, goNext]);

  function onOverlayClick(e) {
    if (e.target === e.currentTarget) {
      onClose();
    }
  }

  function onTouchStart(e) {
    if (e.touches.length !== 1) return;
    touchStartRef.current = { x: e.touches[0].clientX, y: e.touches[0].clientY };
  }
  function onTouchEnd(e) {
    const start = touchStartRef.current;
    if (!start) return;
    touchStartRef.current = null;
    const dx = (e.changedTouches[0]?.clientX ?? start.x) - start.x;
    const dy = (e.changedTouches[0]?.clientY ?? start.y) - start.y;
    if (Math.abs(dx) > 50 && Math.abs(dx) > Math.abs(dy)) {
      if (dx > 0) goPrev();
      else goNext();
    }
  }

  if (!item) return null;

  return (
    <div
      ref={overlayRef}
      className="media-lightbox"
      role="dialog"
      aria-modal="true"
      aria-label={t("lightbox.viewerAria")}
      onClick={onOverlayClick}
      onTouchStart={onTouchStart}
      onTouchEnd={onTouchEnd}
    >
      <button
        type="button"
        className="media-lightbox-close"
        aria-label={t("lightbox.close")}
        onClick={onClose}
      >
        ×
      </button>

      {total > 1 && (
        <>
          <button
            type="button"
            className="media-lightbox-nav prev"
            aria-label={t("lightbox.prev")}
            onClick={goPrev}
          >
            ‹
          </button>
          <button
            type="button"
            className="media-lightbox-nav next"
            aria-label={t("lightbox.next")}
            onClick={goNext}
          >
            ›
          </button>
        </>
      )}

      <figure className="media-lightbox-figure">
        {item.media_kind === "video" ? (
          <video
            key={item.id}
            className="media-lightbox-media"
            src={item.url}
            controls
            autoPlay
            playsInline
          />
        ) : (
          <img
            key={item.id}
            className="media-lightbox-media"
            src={item.url}
            alt={item.caption || ""}
          />
        )}
        <figcaption className="media-lightbox-caption">
          {item.caption ? (
            <span className="media-lightbox-caption-text">{item.caption}</span>
          ) : (
            <span className="media-lightbox-caption-empty" aria-hidden="true" />
          )}
          {total > 1 && (
            <span className="media-lightbox-counter" aria-live="polite">
              {index + 1} / {total}
            </span>
          )}
        </figcaption>
      </figure>
    </div>
  );
}

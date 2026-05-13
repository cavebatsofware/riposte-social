import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { fetchApi } from "../utils/api";
import useFocusTrap from "../utils/useFocusTrap";
import CommentThread from "./CommentThread";
import ReactionBar from "./ReactionBar";
import "./MediaLightbox.css";

/// Full-screen lightbox / carousel for post or album media. Renders one
/// item at a time with prev/next nav, ESC-to-close, swipe on touch, and
/// a caption strip below the media.
///
/// `items`: array of `{ id, url, mime_type, media_kind, caption }` shaped
///   like a `PostMediaResponse` / `AlbumMediaResponse`. Order matters.
/// `index`: the active item index. Owned by the parent.
/// `postId`: id of the parent post (or album, since they're posts now).
///   When supplied, the engagement panel (reactions + comments per media
///   item) renders below the caption. Omit it for read-only contexts
///   where engagement isn't applicable.
/// `onClose`, `onIndex`: parent-supplied callbacks.
export default function MediaLightbox({ items, index, postId, onClose, onIndex }) {
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

  // Captionless images get a localized alt fallback so screen readers
  // hear "Image 2 of 5" instead of an empty announcement when the user
  // pages onto an unannotated photo.
  const fallbackAlt = t("lightbox.imageWithoutCaption", {
    index: index + 1,
    total,
  });
  const imageAlt = item.caption || fallbackAlt;

  // Single live region carries both the position and the caption so an
  // SR user gets the same context a sighted user does on every nav step
  // (visible counter + visible caption). The polite politeness keeps it
  // from interrupting an in-flight comment composer announcement.
  const announcement = item.caption
    ? t("lightbox.indexAnnouncementWithCaption", {
        index: index + 1,
        total,
        caption: item.caption,
      })
    : t("lightbox.indexAnnouncement", { index: index + 1, total });

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
            alt={imageAlt}
          />
        )}
        <figcaption className="media-lightbox-caption">
          {item.caption ? (
            <span className="media-lightbox-caption-text">{item.caption}</span>
          ) : (
            <span className="media-lightbox-caption-empty" aria-hidden="true" />
          )}
          {total > 1 && (
            <span className="media-lightbox-counter" aria-hidden="true">
              {index + 1} / {total}
            </span>
          )}
        </figcaption>
        <span
          className="media-lightbox-sr-announcement"
          aria-live="polite"
          aria-atomic="true"
        >
          {announcement}
        </span>
      </figure>

      {postId && <MediaEngagementPanel postId={postId} mediaId={item.id} />}
    </div>
  );
}

/// Per-item engagement panel below the lightbox media. Lazy-fetches
/// reaction counts + viewer reactions on mount and whenever `mediaId`
/// changes (i.e. the user moves to a different item via prev/next), so
/// the feed and album payloads don't have to ship engagement state for
/// every photo up front.
function MediaEngagementPanel({ postId, mediaId }) {
  const { t } = useTranslation("feed");
  const [state, setState] = useState({
    id: mediaId,
    reaction_counts: {},
    viewer_reaction_kinds: [],
  });

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const r = await fetchApi(
          `/api/posts/${postId}/media/${mediaId}/engagement`,
        );
        if (!r.ok) {
          if (!cancelled) {
            setState({
              id: mediaId,
              reaction_counts: {},
              viewer_reaction_kinds: [],
            });
          }
          return;
        }
        const data = await r.json();
        if (!cancelled) {
          setState({
            id: mediaId,
            reaction_counts: data.reaction_counts || {},
            viewer_reaction_kinds: data.viewer_reaction_kinds || [],
          });
        }
      } catch {
        if (!cancelled) {
          setState({
            id: mediaId,
            reaction_counts: {},
            viewer_reaction_kinds: [],
          });
        }
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [postId, mediaId]);

  return (
    <section
      className="media-lightbox-engagement"
      aria-label={t("mediaEngagement.panelAria")}
    >
      <ReactionBar
        target={{ kind: "media", postId, mediaId }}
        state={state}
        compact
      />
      <CommentThread
        key={mediaId}
        target={{ kind: "media", postId, mediaId }}
      />
    </section>
  );
}

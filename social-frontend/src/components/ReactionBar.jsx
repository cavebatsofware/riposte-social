import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import useRovingFocus from "../utils/useRovingFocus";

/// The Facebook-6 reaction set. Order is the display order in the picker
/// strip. The first entry is the default kind a user gets when they click
/// the bare button without picking. Server-side allowlist is the source
/// of truth — see `src/entities/reaction.rs::ALLOWED_KINDS`.
///
/// `id` and `emoji` stay here (universal across locales); the display
/// label is resolved per-render via `t("feed:reactions.kind.{id}")` so
/// each language gets its own short name ("Like" / "Me gusta" / "赞" …).
export const REACTION_KINDS = [
  { id: "like", emoji: "👍" },
  { id: "love", emoji: "❤️" },
  { id: "haha", emoji: "😂" },
  { id: "wow", emoji: "😮" },
  { id: "sad", emoji: "😢" },
  { id: "angry", emoji: "😡" },
];

const REACTION_BY_ID = REACTION_KINDS.reduce((acc, k) => {
  acc[k.id] = k;
  return acc;
}, {});

const DEFAULT_KIND = REACTION_KINDS[0].id;

/// Reaction button + emoji picker for a post OR a comment.
///
/// Props:
/// - `target`: where reactions go, in one of three shapes:
///     `{ kind: "post", postId }` for `/api/posts/{postId}/reactions`
///     `{ kind: "comment", postId, commentId }` for the comment endpoint
///     `{ kind: "media", postId, mediaId }` for a media item inside a
///       post or album lightbox
/// - `state`: the entity carrying current reaction state
///     `{ id, reaction_counts, viewer_reaction_kinds }`. Both `post::Model`
///     and `comment::Model` payloads expose this shape from the backend.
/// - `compact` (default `false`): tighter sizing for inline use under
///     comments. The picker and summary badges remain visually consistent
///     with the post-level bar; only the trigger button shrinks.
///
/// Behavior:
/// - Click toggles the user's current kind (defaults to `like` when none).
/// - Hover (desktop) or long-press (mobile, ≥400ms) reveals the picker.
/// - Selecting a different kind swaps (DELETE old + POST new) so a user
///   only ever has one kind per target — same constraint Facebook
///   enforces.
/// - Optimistic update flips local state immediately; server response is
///   authoritative on success; failure rolls back to the prop snapshot.
///
/// In feed PostCards the parent wraps the article in a react-router
/// `<Link>`; click handlers `preventDefault()` + `stopPropagation()` so
/// reacting doesn't navigate.
export default function ReactionBar({ target, state, compact = false }) {
  const { user } = useAuth();
  const { t } = useTranslation("feed");
  const [counts, setCounts] = useState(state.reaction_counts || {});
  const [viewerKinds, setViewerKinds] = useState(
    state.viewer_reaction_kinds || [],
  );
  const [pending, setPending] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const longPressTimer = useRef(null);
  const wrapperRef = useRef(null);
  const triggerRef = useRef(null);
  const pickerRef = useRef(null);

  // Roving focus inside the picker once it's open. Horizontal layout;
  // ArrowLeft/ArrowRight (and Home/End) move the tab anchor + focus.
  // Disabled when the picker is closed so the inactive buttons don't
  // claim a tab stop.
  useRovingFocus(pickerRef, pickerOpen, { orientation: "horizontal" });

  useEffect(() => {
    setCounts(state.reaction_counts || {});
    setViewerKinds(state.viewer_reaction_kinds || []);
  }, [state.id, state.reaction_counts, state.viewer_reaction_kinds]);

  // Close the picker when the user taps outside (mobile path; desktop
  // hover-out CSS already handles the close) or when keyboard focus
  // moves out of the bar entirely. Escape inside the picker returns
  // focus to the trigger.
  useEffect(() => {
    if (!pickerOpen) return;
    function onDocPointer(e) {
      if (!wrapperRef.current) return;
      if (!wrapperRef.current.contains(e.target)) {
        setPickerOpen(false);
      }
    }
    function onFocusIn(e) {
      if (!wrapperRef.current) return;
      if (!wrapperRef.current.contains(e.target)) {
        setPickerOpen(false);
      }
    }
    function onKey(e) {
      if (e.key === "Escape") {
        setPickerOpen(false);
        triggerRef.current?.focus();
      }
    }
    document.addEventListener("pointerdown", onDocPointer);
    document.addEventListener("focusin", onFocusIn);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDocPointer);
      document.removeEventListener("focusin", onFocusIn);
      document.removeEventListener("keydown", onKey);
    };
  }, [pickerOpen]);

  const currentKind = viewerKinds[0] || null;
  const primary = currentKind ? REACTION_BY_ID[currentKind] : null;
  // Resolved labels per-kind from the active locale. Computed once per
  // render so the picker, summary badges, and aria-labels all share one
  // pass over REACTION_KINDS.
  const labelOf = (id) => t(`reactions.kind.${id}`);
  const totalCount = REACTION_KINDS.reduce(
    (sum, k) => sum + (counts[k.id] || 0),
    0,
  );
  const canReact = Boolean(user);

  // Build the base URL once per target. Same shape on every endpoint:
  // POST <base> with { kind } → add; DELETE <base>/{kind} → remove.
  let baseUrl;
  if (target.kind === "post") {
    baseUrl = `/api/posts/${target.postId}/reactions`;
  } else if (target.kind === "media") {
    baseUrl = `/api/posts/${target.postId}/media/${target.mediaId}/reactions`;
  } else {
    baseUrl = `/api/posts/${target.postId}/comments/${target.commentId}/reactions`;
  }

  async function applyReaction(nextKind) {
    if (!canReact || pending) return;
    setPickerOpen(false);
    const wasKind = currentKind;
    if (wasKind === nextKind) {
      await runRequest({ remove: wasKind });
      return;
    }
    await runRequest({ remove: wasKind, add: nextKind });
  }

  async function toggleDefault(e) {
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }
    if (!canReact || pending) return;
    if (currentKind) {
      await applyReaction(currentKind);
    } else {
      await applyReaction(DEFAULT_KIND);
    }
  }

  async function runRequest({ remove, add }) {
    const optimisticCounts = { ...counts };
    let optimisticKinds = [...viewerKinds];
    if (remove) {
      optimisticCounts[remove] = Math.max(0, (optimisticCounts[remove] || 0) - 1);
      if (optimisticCounts[remove] === 0) delete optimisticCounts[remove];
      optimisticKinds = optimisticKinds.filter((k) => k !== remove);
    }
    if (add) {
      optimisticCounts[add] = (optimisticCounts[add] || 0) + 1;
      optimisticKinds = [add, ...optimisticKinds.filter((k) => k !== add)];
    }
    setPending(true);
    setCounts(optimisticCounts);
    setViewerKinds(optimisticKinds);

    try {
      let lastResponse = null;
      if (remove) {
        const r = await fetchApi(`${baseUrl}/${remove}`, { method: "DELETE" });
        if (!r.ok) throw new Error("reaction remove failed");
        lastResponse = await r.json();
      }
      if (add) {
        const r = await fetchApi(baseUrl, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ kind: add }),
        });
        if (!r.ok) throw new Error("reaction add failed");
        lastResponse = await r.json();
      }
      if (lastResponse) {
        setCounts(lastResponse.reaction_counts || {});
        setViewerKinds(lastResponse.viewer_reaction_kinds || []);
      }
    } catch {
      setCounts(state.reaction_counts || {});
      setViewerKinds(state.viewer_reaction_kinds || []);
    } finally {
      setPending(false);
    }
  }

  function handlePointerDown(e) {
    if (e.pointerType === "touch") {
      longPressTimer.current = setTimeout(() => {
        setPickerOpen(true);
      }, 400);
    }
  }
  function clearLongPress() {
    if (longPressTimer.current) {
      clearTimeout(longPressTimer.current);
      longPressTimer.current = null;
    }
  }

  return (
    <div
      ref={wrapperRef}
      className={`reaction-bar ${compact ? "compact" : ""} ${pickerOpen ? "picker-open" : ""}`}
      onPointerLeave={() => setPickerOpen(false)}
    >
      {canReact ? (
        <button
          ref={triggerRef}
          type="button"
          className={`reaction-btn ${currentKind ? "active" : ""}`}
          onClick={toggleDefault}
          onPointerDown={handlePointerDown}
          onPointerUp={clearLongPress}
          onPointerCancel={clearLongPress}
          onPointerMove={clearLongPress}
          onMouseEnter={() => setPickerOpen(true)}
          onKeyDown={(e) => {
            // Keyboard equivalent of hover/long-press: ArrowDown / ArrowUp
            // open the picker so the 6-emoji set is reachable without a
            // mouse. useRovingFocus moves focus to the first picker
            // button as the picker activates.
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              e.stopPropagation();
              setPickerOpen(true);
            }
          }}
          disabled={pending}
          aria-haspopup="menu"
          aria-expanded={pickerOpen}
          aria-pressed={Boolean(currentKind)}
          aria-label={
            primary
              ? t("reactions.reactedWith", { kind: labelOf(primary.id) })
              : t("reactions.react")
          }
        >
          <span className="reaction-icon" aria-hidden="true">
            {primary ? primary.emoji : REACTION_KINDS[0].emoji}
          </span>
          <span className="reaction-label">
            {primary ? labelOf(primary.id) : t("reactions.react")}
          </span>
        </button>
      ) : (
        totalCount > 0 && (
          <span
            role="img"
            className="reaction-static"
            aria-label={t("reactions.summaryAria", { count: totalCount })}
          >
            <span className="reaction-icon" aria-hidden="true">
              👍
            </span>
          </span>
        )
      )}

      <span className="reaction-summary" aria-hidden={totalCount === 0}>
        {REACTION_KINDS.filter((k) => (counts[k.id] || 0) > 0).map((k) => (
          <span
            key={k.id}
            className="reaction-summary-badge"
            title={`${labelOf(k.id)}: ${counts[k.id]}`}
          >
            <span aria-hidden="true">{k.emoji}</span>
            <span className="reaction-summary-count">{counts[k.id]}</span>
          </span>
        ))}
      </span>

      {canReact && (
        <div
          ref={pickerRef}
          className="reaction-picker"
          role="menu"
          aria-label={t("reactions.pickerAria")}
        >
          {REACTION_KINDS.map((k) => (
            <button
              key={k.id}
              type="button"
              role="menuitem"
              className={`reaction-picker-btn ${
                currentKind === k.id ? "active" : ""
              }`}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                applyReaction(k.id);
              }}
              disabled={pending}
              aria-label={t("reactions.reactWithAria", { kind: labelOf(k.id) })}
              title={labelOf(k.id)}
            >
              <span aria-hidden="true">{k.emoji}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

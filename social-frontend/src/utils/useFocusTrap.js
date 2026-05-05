import { useEffect, useRef } from "react";

/// Tab focus inside the element pointed to by the returned ref while the
/// trap is `active`. Escape calls `onEscape`. When the trap goes from
/// active to inactive (or unmounts while active), focus is restored to
/// whatever element had focus at activation time typically the modal /
/// drawer / popover trigger.
///
/// Designed for dialogs, drawers, popovers, lightboxes anywhere the
/// keyboard user must not be able to tab out of the overlay while it's
/// open. The host component owns visibility state and Escape handling
/// (this hook just delivers the event); it does not render anything.
///
/// Usage:
///   const ref = useFocusTrap(open, { onEscape: () => setOpen(false) });
///   return <div ref={ref} role="dialog" aria-modal="true">...</div>;
///
/// On `active` going true: snapshots `document.activeElement` as the
/// restore target, then moves focus to the first focusable element
/// inside the container (or the container itself if it's tabbable).
///
/// While active: a Tab listener wraps focus around the focusable list.
/// An Escape listener fires `onEscape` if provided.
///
/// On `active` going false (or unmount during active): restores focus
/// to the snapshot. If the snapshot is no longer in the DOM, focus
/// falls through to `<body>`, which is the browser default.
///
/// `restoreFocus = false` opts out of focus restoration useful when
/// the closing action navigates away from the page and restoration
/// would target a stale element.
const FOCUSABLE_SELECTOR = [
  "a[href]",
  "area[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type=\"hidden\"])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "iframe",
  "object",
  "embed",
  "audio[controls]",
  "video[controls]",
  "summary",
  "[contenteditable]:not([contenteditable=\"false\"])",
  "[tabindex]:not([tabindex=\"-1\"])",
].join(",");

function getFocusable(container) {
  if (!container) return [];
  const nodes = container.querySelectorAll(FOCUSABLE_SELECTOR);
  return Array.from(nodes).filter((el) => {
    if (el.hasAttribute("disabled")) return false;
    if (el.getAttribute("aria-hidden") === "true") return false;
    // hidden (display:none / visibility:hidden) elements aren't
    // focusable; offsetParent === null catches both for non-fixed nodes.
    if (el.offsetParent === null && el.getClientRects().length === 0) {
      return false;
    }
    return true;
  });
}

export function useFocusTrap(active, { onEscape, restoreFocus = true } = {}) {
  const containerRef = useRef(null);
  const restoreRef = useRef(null);
  const onEscapeRef = useRef(onEscape);

  useEffect(() => {
    onEscapeRef.current = onEscape;
  }, [onEscape]);

  useEffect(() => {
    if (!active) return undefined;
    const container = containerRef.current;
    if (!container) return undefined;

    restoreRef.current =
      restoreFocus && document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;

    // Move focus into the trap. Prefer the first focusable child;
    // fall back to the container itself with tabindex=-1 so screen
    // readers announce the dialog rather than landing on body.
    const initial = getFocusable(container);
    let addedTabindex = false;
    if (initial.length > 0) {
      initial[0].focus();
    } else {
      if (!container.hasAttribute("tabindex")) {
        container.setAttribute("tabindex", "-1");
        addedTabindex = true;
      }
      container.focus();
    }

    function onKeyDown(e) {
      if (e.key === "Escape" && onEscapeRef.current) {
        e.stopPropagation();
        onEscapeRef.current(e);
        return;
      }
      if (e.key !== "Tab") return;
      const focusable = getFocusable(container);
      if (focusable.length === 0) {
        e.preventDefault();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const activeEl = document.activeElement;
      if (e.shiftKey) {
        if (activeEl === first || !container.contains(activeEl)) {
          e.preventDefault();
          last.focus();
        }
      } else if (activeEl === last) {
        e.preventDefault();
        first.focus();
      }
    }

    container.addEventListener("keydown", onKeyDown);
    return () => {
      container.removeEventListener("keydown", onKeyDown);
      if (addedTabindex) {
        container.removeAttribute("tabindex");
      }
      if (restoreFocus && restoreRef.current && document.contains(restoreRef.current)) {
        restoreRef.current.focus();
      }
      restoreRef.current = null;
    };
  }, [active, restoreFocus]);

  return containerRef;
}

export default useFocusTrap;

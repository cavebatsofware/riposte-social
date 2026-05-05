import { useEffect } from "react";

/// Wire roving-focus keyboard navigation onto a popover container.
///
/// Listens for Arrow / Home / End on the container element while
/// `active` is true and moves focus across the matching descendants
/// (default: any element with a `[role="menuitem"]`, `[role="menuitemradio"]`,
/// or `[role="menuitemcheckbox"]`).
///
/// On activation the first item receives focus so screen reader users
/// land inside the menu without an extra Tab. Tab is left to the
/// browser; consumers that need a focus trap should compose with
/// `useFocusTrap`.
export function useRovingFocus(
  containerRef,
  active,
  {
    selector = '[role="menuitem"], [role="menuitemradio"], [role="menuitemcheckbox"]',
    orientation = "vertical",
    wrap = true,
  } = {},
) {
  useEffect(() => {
    if (!active) return undefined;
    const container = containerRef.current;
    if (!container) return undefined;

    function items() {
      return Array.from(container.querySelectorAll(selector)).filter(
        (el) => !el.hasAttribute("disabled"),
      );
    }

    const initial = items();
    if (initial.length > 0) initial[0].focus();

    const nextKey = orientation === "horizontal" ? "ArrowRight" : "ArrowDown";
    const prevKey = orientation === "horizontal" ? "ArrowLeft" : "ArrowUp";

    function onKeyDown(e) {
      if (![nextKey, prevKey, "Home", "End"].includes(e.key)) return;
      const list = items();
      if (list.length === 0) return;
      const current = document.activeElement;
      const idx = list.indexOf(current);
      let next;
      if (e.key === "Home") {
        next = 0;
      } else if (e.key === "End") {
        next = list.length - 1;
      } else if (e.key === nextKey) {
        next = idx + 1;
        if (next >= list.length) next = wrap ? 0 : list.length - 1;
      } else {
        next = idx - 1;
        if (next < 0) next = wrap ? list.length - 1 : 0;
      }
      e.preventDefault();
      list[next].focus();
    }

    container.addEventListener("keydown", onKeyDown);
    return () => {
      container.removeEventListener("keydown", onKeyDown);
    };
  }, [active, containerRef, selector, orientation, wrap]);
}

export default useRovingFocus;

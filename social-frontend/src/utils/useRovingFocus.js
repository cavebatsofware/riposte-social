import { useEffect } from "react";

/// Wire roving-focus keyboard navigation onto a popover container.
///
/// Listens for Arrow / Home / End on the container element while
/// `active` is true and moves focus across the matching descendants
/// (default: any element with a `[role="menuitem"]`, `[role="menuitemradio"]`,
/// or `[role="menuitemcheckbox"]`).
///
/// Implements the WAI-ARIA menu pattern's roving tabindex: only one
/// item is in the tab order at a time (`tabindex="0"`), and arrow keys
/// move both DOM focus and the tab-order anchor across items. Tab from
/// inside the menu therefore exits to the next focusable element in the
/// document instead of cycling through siblings.
///
/// On activation the active item (or the first item if none is marked
/// active) receives focus so screen reader users land inside the menu
/// without an extra Tab. Consumers that need a focus trap should
/// compose with `useFocusTrap`.
export function useRovingFocus(
  containerRef,
  active,
  {
    selector = '[role="menuitem"], [role="menuitemradio"], [role="menuitemcheckbox"]',
    orientation = "vertical",
    wrap = true,
    autoFocus = true,
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

    function setRovingTabIndex(list, focusedIndex) {
      list.forEach((el, i) => {
        el.tabIndex = i === focusedIndex ? 0 : -1;
      });
    }

    const initial = items();
    if (initial.length > 0) {
      // Anchor the tab order on the first menuitemradio that's already
      // checked, otherwise the first item.
      const checkedIdx = initial.findIndex(
        (el) => el.getAttribute("aria-checked") === "true",
      );
      const startIdx = checkedIdx >= 0 ? checkedIdx : 0;
      setRovingTabIndex(initial, startIdx);
      if (autoFocus) initial[startIdx].focus();
    }

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
      setRovingTabIndex(list, next);
      list[next].focus();
    }

    container.addEventListener("keydown", onKeyDown);
    return () => {
      container.removeEventListener("keydown", onKeyDown);
      // Restore items to default tab order on cleanup so the next mount
      // starts from a clean slate.
      const list = items();
      list.forEach((el) => {
        el.removeAttribute("tabindex");
      });
    };
  }, [active, containerRef, selector, orientation, wrap, autoFocus]);
}

export default useRovingFocus;

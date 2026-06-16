import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { patchPostVisibility } from "../features/feed/api";
import { useRovingFocus } from "@cavebatsofware/riposte-design-system/shared";

const OPTION_IDS = ["private", "public", "commenters", "posters"];

/// Inline visibility dropdown for the post-meta line. Author or admin
/// only; the parent decides whether to render this or the read-only
/// `<VisibilityBadge>`. Selecting an option PATCHes
/// `/api/posts/{id}` and calls `onChange(newVisibility)` so the parent
/// can sync any cached copy.
export default function VisibilityMenu({ post, onChange }: { post: { id: string; visibility: string }; onChange?: (v: string) => void }) {
  const { t } = useTranslation("feed");
  const [open, setOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  // Local optimistic-update state that follows post.visibility from the
  // parent. Synced via useLayoutEffect so we never paint stale data
  // after the parent commits the new value.
  const [current, setCurrent] = useState(post.visibility);
  useLayoutEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- useLayoutEffect is the correct pattern for syncing derived state before paint; rule fires unconditionally
    setCurrent(post.visibility);
  }, [post.visibility]);
  const wrapperRef = useRef(null);
  const triggerRef = useRef(null);
  const popoverRef = useRef(null);

  useRovingFocus(popoverRef, open);

  useEffect(() => {
    if (!open) return undefined;
    function onDocClick(e) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target)) {
        setOpen(false);
      }
    }
    function onFocusOut(e) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target)) {
        setOpen(false);
      }
    }
    function onKey(e) {
      if (e.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    }
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("focusin", onFocusOut);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("focusin", onFocusOut);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  async function selectOption(id) {
    if (id === current || submitting) {
      setOpen(false);
      return;
    }
    setSubmitting(true);
    setError("");
    try {
      const response = await patchPostVisibility(post.id, id);
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("visibility.menuFailed"));
      }
      setCurrent(id);
      setOpen(false);
      if (onChange) onChange(id);
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  const labelKey = OPTION_IDS.includes(current) ? current : "public";
  const label = t(`visibility.${labelKey}.name`);

  return (
    <div
      className={`visibility-menu ${open ? "menu-open" : ""}`}
      ref={wrapperRef}
    >
      <button
        ref={triggerRef}
        type="button"
        className={`visibility-badge visibility-badge-button ${current}`}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={submitting}
        onClick={() => setOpen((v) => !v)}
      >
        {label}
        <span className="visibility-menu-chevron" aria-hidden="true">
          ▾
        </span>
      </button>
      {/* eslint-disable-next-line a11yinspect/menu-element-warning -- menuitem children rendered via map; static analyzer cannot traverse */}
      <div ref={popoverRef} className="visibility-menu-popover" role="menu" tabIndex={-1}>
        {OPTION_IDS.map((id) => (
          <button
            key={id}
            type="button"
            role="menuitem"
            className={`visibility-menu-item ${id === current ? "active" : ""}`}
            disabled={submitting}
            onClick={() => selectOption(id)}
          >
            <span className={`visibility-menu-swatch ${id}`} aria-hidden="true" />
            {t(`visibility.${id}.name`)}
            {id === current && (
              <span className="visibility-menu-check" aria-hidden="true">
                ✓
              </span>
            )}
          </button>
        ))}
        {error && <div className="visibility-menu-error">{error}</div>}
      </div>
    </div>
  );
}

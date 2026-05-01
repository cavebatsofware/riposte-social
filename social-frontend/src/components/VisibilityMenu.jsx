import { useEffect, useRef, useState } from "react";
import { fetchApi } from "../utils/api";

const OPTIONS = [
  { id: "private", label: "Private" },
  { id: "public", label: "Public" },
  { id: "commenters", label: "Commenters" },
  { id: "posters", label: "Posters" },
];

/// Inline dropdown that lets a post's author or an administrator change
/// the post's visibility from anywhere it's rendered (feed card, permalink,
/// etc.) without entering the Compose edit flow. The trigger is the
/// existing `<VisibilityBadge>`, just with an added chevron and click
/// handler.
///
/// Backend: PATCH /api/posts/{id} accepts a partial body of just
/// `{visibility: "..."}`; the change is applied transactionally and the
/// updated post is returned. We update local state on success and call
/// `onChange(newVisibility)` so the parent can sync any cached copy.
///
/// Wrapped in `e.stopPropagation()` calls because the parent PostCard
/// renders the whole card inside a `<Link>` to the permalink on the feed
/// variant; without stopPropagation, opening the menu would navigate.
export default function VisibilityMenu({ post, onChange }) {
  const [open, setOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [current, setCurrent] = useState(post.visibility);
  const wrapperRef = useRef(null);

  useEffect(() => {
    setCurrent(post.visibility);
  }, [post.visibility]);

  useEffect(() => {
    if (!open) return undefined;
    function onDocClick(e) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target)) {
        setOpen(false);
      }
    }
    function onKey(e) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  async function selectOption(e, id) {
    e.preventDefault();
    e.stopPropagation();
    if (id === current || submitting) {
      setOpen(false);
      return;
    }
    setSubmitting(true);
    setError("");
    try {
      const response = await fetchApi(`/api/posts/${post.id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ visibility: id }),
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to update visibility");
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

  const label =
    current === "private"
      ? "Private"
      : current === "commenters"
        ? "Commenters"
        : current === "posters"
          ? "Posters"
          : "Public";

  return (
    <span className="visibility-menu" ref={wrapperRef}>
      <button
        type="button"
        className={`visibility-badge visibility-badge-button ${current}`}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={submitting}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setOpen((v) => !v);
        }}
      >
        {label}
        <span className="visibility-menu-chevron" aria-hidden="true">
          ▾
        </span>
      </button>
      {open && (
        <div className="visibility-menu-popover" role="menu">
          {OPTIONS.map((o) => (
            <button
              key={o.id}
              type="button"
              role="menuitem"
              className={`visibility-menu-item ${o.id === current ? "active" : ""}`}
              disabled={submitting}
              onClick={(e) => selectOption(e, o.id)}
            >
              <span className={`visibility-menu-swatch ${o.id}`} aria-hidden="true" />
              {o.label}
              {o.id === current && (
                <span className="visibility-menu-check" aria-hidden="true">
                  ✓
                </span>
              )}
            </button>
          ))}
          {error && <div className="visibility-menu-error">{error}</div>}
        </div>
      )}
    </span>
  );
}

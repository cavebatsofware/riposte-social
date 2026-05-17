/* global React */
const { useState: useStateRB, useRef: useRefRB, useEffect: useEffectRB } = React;

/// Facebook-6 reaction set. Order is canonical.
const REACTION_KINDS = [
  { id: "like",  emoji: "👍", label: "Like" },
  { id: "love",  emoji: "❤️", label: "Love" },
  { id: "haha",  emoji: "😂", label: "Haha" },
  { id: "wow",   emoji: "😮", label: "Wow" },
  { id: "sad",   emoji: "😢", label: "Sad" },
  { id: "angry", emoji: "😡", label: "Angry" },
];
const REACTION_BY_ID = REACTION_KINDS.reduce((m, k) => { m[k.id] = k; return m; }, {});

/// Reaction button + emoji picker for a post.
function ReactionBar({ counts: initialCounts, viewerKinds: initialViewer }) {
  const [counts, setCounts] = useStateRB(initialCounts || {});
  const [viewerKinds, setViewerKinds] = useStateRB(initialViewer || []);
  const [pickerOpen, setPickerOpen] = useStateRB(false);
  const wrapperRef = useRefRB(null);
  const currentKind = viewerKinds[0] || null;
  const primary = currentKind ? REACTION_BY_ID[currentKind] : null;

  useEffectRB(() => {
    if (!pickerOpen) return;
    function onDown(e) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target)) setPickerOpen(false);
    }
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [pickerOpen]);

  function apply(nextKind) {
    setPickerOpen(false);
    const wasKind = currentKind;
    const nextCounts = { ...counts };
    let nextViewer = [...viewerKinds];
    if (wasKind === nextKind) {
      nextCounts[wasKind] = Math.max(0, (nextCounts[wasKind] || 0) - 1);
      if (nextCounts[wasKind] === 0) delete nextCounts[wasKind];
      nextViewer = nextViewer.filter((k) => k !== wasKind);
    } else {
      if (wasKind) {
        nextCounts[wasKind] = Math.max(0, (nextCounts[wasKind] || 0) - 1);
        if (nextCounts[wasKind] === 0) delete nextCounts[wasKind];
        nextViewer = nextViewer.filter((k) => k !== wasKind);
      }
      nextCounts[nextKind] = (nextCounts[nextKind] || 0) + 1;
      nextViewer = [nextKind, ...nextViewer];
    }
    setCounts(nextCounts);
    setViewerKinds(nextViewer);
  }

  function toggleDefault(e) {
    e.preventDefault(); e.stopPropagation();
    apply(currentKind || REACTION_KINDS[0].id);
  }

  return (
    <div
      ref={wrapperRef}
      className={`reaction-bar ${pickerOpen ? "picker-open" : ""}`}
      onPointerLeave={() => setPickerOpen(false)}
    >
      <button
        type="button"
        className={`reaction-btn ${currentKind ? "active" : ""}`}
        onClick={toggleDefault}
        onMouseEnter={() => setPickerOpen(true)}
        aria-haspopup="menu"
        aria-expanded={pickerOpen}
        aria-pressed={Boolean(currentKind)}
        aria-label={primary ? `Reacted with ${primary.label}` : "React"}
      >
        <span className="reaction-icon" aria-hidden="true">{primary ? primary.emoji : "👍"}</span>
        <span>{primary ? primary.label : "React"}</span>
      </button>

      <span className="reaction-summary" aria-hidden={Object.keys(counts).length === 0}>
        {REACTION_KINDS.filter((k) => (counts[k.id] || 0) > 0).map((k) => (
          <span key={k.id} className="reaction-summary-badge" title={`${k.label}: ${counts[k.id]}`}>
            <span aria-hidden="true">{k.emoji}</span>
            <span>{counts[k.id]}</span>
          </span>
        ))}
      </span>

      <div className="reaction-picker" role="menu" aria-label="Pick a reaction">
        {REACTION_KINDS.map((k) => (
          <button
            key={k.id}
            type="button"
            role="menuitem"
            className={`reaction-picker-btn ${currentKind === k.id ? "active" : ""}`}
            onClick={(e) => { e.preventDefault(); e.stopPropagation(); apply(k.id); }}
            title={k.label}
            aria-label={`React with ${k.label}`}
          >
            <span aria-hidden="true">{k.emoji}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

Object.assign(window, { ReactionBar, REACTION_KINDS });

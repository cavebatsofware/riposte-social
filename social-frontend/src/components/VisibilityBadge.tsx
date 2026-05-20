import { useId } from "react";
import { useTranslation } from "react-i18next";

const KNOWN_TIERS = ["private", "commenters", "posters", "user_list"];

/// Read-only visibility badge used by post cards, album hero, and any
/// other component that surfaces an effective-visibility label.
///
/// When `fromCategory` is true the row's effective visibility is
/// inherited from its parent category and the "from category" provenance
/// is surfaced via an ARIA tooltip on hover.
///
/// `visibility` accepts the four post-level tiers plus the category-only
/// `user_list` tier.
export default function VisibilityBadge({ visibility, fromCategory }) {
  const { t } = useTranslation("feed");
  const tooltipId = useId();
  const cls = `visibility-badge ${visibility}`;
  const key = KNOWN_TIERS.includes(visibility) ? visibility : "public";
  const label = t(`visibility.${key}.name`);
  if (!fromCategory) {
    return <span className={cls}>{label}</span>;
  }
  // The badge is made keyboard-focusable so the tooltip is reachable
  // without a mouse. CSS reveals the tooltip on `:focus-within` of the
  // wrapping span; the badge itself stays a non-interactive label
  // (no role change), it just acquires a tab stop. The lint rule
  // discourages `tabIndex` on non-interactive elements, but a focusable
  // tooltip trigger is the documented WAI-ARIA pattern for surfacing
  // supplementary labels to keyboard users.
  return (
    <span className="visibility-badge-wrap">
      <span
        className={cls}
        // eslint-disable-next-line a11yinspect/focus-element-warning
        tabIndex={0}
        aria-describedby={tooltipId}
      >
        {label}
      </span>
      <span className="visibility-tooltip" role="tooltip" id={tooltipId}>
        {t("visibility.fromCategory")}
      </span>
    </span>
  );
}
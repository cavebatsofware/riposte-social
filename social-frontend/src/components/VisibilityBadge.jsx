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
  return (
    <span className="visibility-badge-wrap">
      <span className={cls} aria-describedby={tooltipId}>
        {label}
      </span>
      <span className="visibility-tooltip" role="tooltip" id={tooltipId}>
        {t("visibility.fromCategory")}
      </span>
    </span>
  );
}
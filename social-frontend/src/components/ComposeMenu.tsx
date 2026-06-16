import { useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { PopoverPicker } from "@cavebatsofware/riposte-design-system/shared";
import { useRovingFocus } from "@cavebatsofware/riposte-design-system/shared";

interface ComposeLink {
  to: string;
  label: string;
}

/// Compose dropdown that gathers the per-kind composers (post / article /
/// album) under a single trigger. Built on the shared PopoverPicker shell
/// (same chassis as LanguagePicker / ThemePicker) so open/close, focus
/// trap, and inline variant come for free. Items are rendered as
/// `role=menuitem` Links with `useRovingFocus` for arrow-key navigation.
export default function ComposeMenu({
  variant = "popover",
  links,
}: {
  variant?: "popover" | "inline";
  links: ComposeLink[];
}) {
  const { t } = useTranslation("common");
  const [open, setOpen] = useState(false);
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const inlineRef = useRef<HTMLDivElement | null>(null);

  useRovingFocus(popoverRef, variant === "popover" && open);
  useRovingFocus(inlineRef, variant === "inline", { autoFocus: false });

  function pick() {
    if (variant === "popover") setOpen(false);
  }

  const list = (
    // eslint-disable-next-line a11yinspect/menu-element-warning -- menuitem children rendered via map; static analyzer cannot traverse
    <div
      className="compose-picker-list"
      role="menu"
      tabIndex={-1}
      aria-label={t("nav.compose")}
    >
      <div className="compose-picker-title" role="presentation">
        {t("nav.compose")}
      </div>
      {links.map((l) => (
        // eslint-disable-next-line a11yinspect/focus-element-warning -- tabIndex managed imperatively by useRovingFocus; static analyzer cannot detect it
        <Link
          key={l.to}
          to={l.to}
          role="menuitem"
          className="compose-picker-item"
          onClick={pick}
        >
          {l.label}
        </Link>
      ))}
    </div>
  );

  return (
    <PopoverPicker
      variant={variant}
      open={open}
      onOpenChange={setOpen}
      className="compose-picker"
      toggleAriaLabel={t("nav.composeMenuAria")}
      popoverAriaLabel={t("nav.compose")}
      popoverRef={popoverRef}
      inlineRef={inlineRef}
      toggleIcon={
        <>
          <span>{t("nav.compose")}</span>
          <span aria-hidden="true" className="compose-picker-caret">
            ▾
          </span>
        </>
      }
    >
      {list}
    </PopoverPicker>
  );
}

import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  PopoverPicker,
  useRovingFocus,
} from "@cavebatsofware/riposte-design-system/shared";

/// Visitor-facing share menu for a post or article. Available to anyone
/// who can already see the content, signed in or not: every target is
/// client-side (Web Share API, platform intent URLs, `mailto:`/`sms:`,
/// clipboard), so nothing here writes to the server and it works for
/// anonymous viewers. Built on the shared PopoverPicker (same chassis as
/// ComposeMenu) so open/close, focus trap, and outside-click come for
/// free; items are `role=menuitem` buttons with `useRovingFocus` for
/// arrow-key navigation.
///
/// `isPublic` gates the reach: a link only helps if the recipient can
/// open it, so external and native-share targets appear only when the
/// content's effective visibility is `public`. Copy-link stays available
/// to any viewer who can see the item.
export default function ShareMenu({
  path,
  title,
  isPublic,
}: {
  path: string;
  title?: string;
  isPublic: boolean;
}) {
  const { t } = useTranslation("common");
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState<null | "ok" | "fail">(null);
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const copiedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useRovingFocus(popoverRef, open);

  const url =
    typeof window !== "undefined" ? window.location.origin + path : path;
  const text = title || t("siteName");
  const enc = encodeURIComponent;

  function flashCopied(state: "ok" | "fail") {
    setCopied(state);
    if (copiedTimer.current) clearTimeout(copiedTimer.current);
    copiedTimer.current = setTimeout(() => setCopied(null), 2500);
  }

  async function copyLink() {
    setOpen(false);
    try {
      await navigator.clipboard.writeText(url);
      flashCopied("ok");
    } catch {
      flashCopied("fail");
    }
  }

  async function nativeShare() {
    setOpen(false);
    try {
      await navigator.share({ title: text, url });
    } catch {
      // The user dismissing the native sheet rejects with AbortError;
      // that is a normal cancel, not an error to surface.
    }
  }

  function openTarget(href: string) {
    setOpen(false);
    window.open(href, "_blank", "noopener,noreferrer");
  }

  // mailto:/sms: are handled by the OS without unloading the SPA, so
  // navigating the current window is the reliable path (a popup blocker
  // eats window.open on these schemes in some browsers).
  function openScheme(href: string) {
    setOpen(false);
    window.location.href = href;
  }

  function shareMastodon() {
    const raw = window.prompt(t("share.mastodonPrompt"));
    if (!raw) return;
    const instance = raw.trim().replace(/^https?:\/\//, "").replace(/\/+$/, "");
    if (!instance) return;
    openTarget(`https://${instance}/share?text=${enc(`${text} ${url}`)}`);
  }

  const hasNativeShare =
    typeof navigator !== "undefined" && typeof navigator.share === "function";

  return (
    <div className="share-picker-wrap">
      <PopoverPicker
        open={open}
        onOpenChange={setOpen}
        className="share-picker"
        toggleAriaLabel={t("share.triggerAria")}
        popoverAriaLabel={t("share.menuAria")}
        popoverRef={popoverRef}
        toggleIcon={
          <>
            <ShareIcon />
            <span>{t("share.trigger")}</span>
          </>
        }
      >
        <div
          className="share-picker-list"
          role="menu"
          tabIndex={-1}
          aria-label={t("share.menuAria")}
        >
          <button
            type="button"
            role="menuitem"
            className="share-picker-item"
            onClick={copyLink}
          >
            {t("share.copyLink")}
          </button>

          {isPublic && hasNativeShare && (
            <button
              type="button"
              role="menuitem"
              className="share-picker-item"
              onClick={nativeShare}
            >
              {t("share.nativeShare")}
            </button>
          )}

          {isPublic && (
            <>
              <button
                type="button"
                role="menuitem"
                className="share-picker-item"
                onClick={() => openScheme(`mailto:?subject=${enc(text)}&body=${enc(url)}`)}
              >
                {t("share.email")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="share-picker-item"
                onClick={() => openScheme(`sms:?&body=${enc(`${text} ${url}`)}`)}
              >
                {t("share.sms")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="share-picker-item"
                onClick={() =>
                  openTarget(
                    `https://twitter.com/intent/tweet?url=${enc(url)}&text=${enc(text)}`,
                  )
                }
              >
                {t("share.targets.x")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="share-picker-item"
                onClick={() =>
                  openTarget(`https://www.facebook.com/sharer/sharer.php?u=${enc(url)}`)
                }
              >
                {t("share.targets.facebook")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="share-picker-item"
                onClick={() =>
                  openTarget(
                    `https://www.linkedin.com/sharing/share-offsite/?url=${enc(url)}`,
                  )
                }
              >
                {t("share.targets.linkedin")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="share-picker-item"
                onClick={() =>
                  openTarget(
                    `https://www.reddit.com/submit?url=${enc(url)}&title=${enc(text)}`,
                  )
                }
              >
                {t("share.targets.reddit")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="share-picker-item"
                onClick={() => openTarget(`https://wa.me/?text=${enc(`${text} ${url}`)}`)}
              >
                {t("share.targets.whatsapp")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="share-picker-item"
                onClick={() =>
                  openTarget(`https://t.me/share/url?url=${enc(url)}&text=${enc(text)}`)
                }
              >
                {t("share.targets.telegram")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="share-picker-item"
                onClick={shareMastodon}
              >
                {t("share.targets.mastodon")}
              </button>
            </>
          )}
        </div>
      </PopoverPicker>
      <span
        className="share-picker-status"
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        {copied === "ok"
          ? t("share.linkCopied")
          : copied === "fail"
            ? t("share.copyFailed")
            : ""}
      </span>
    </div>
  );
}

/// Inline share glyph. Decorative: the toggle's accessible name comes
/// from PopoverPicker's `toggleAriaLabel` and the adjacent text label.
function ShareIcon() {
  return (
    <svg
      className="share-picker-icon"
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <circle cx="18" cy="5" r="3" />
      <circle cx="6" cy="12" r="3" />
      <circle cx="18" cy="19" r="3" />
      <line x1="8.6" y1="10.5" x2="15.4" y2="6.5" />
      <line x1="8.6" y1="13.5" x2="15.4" y2="17.5" />
    </svg>
  );
}

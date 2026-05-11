import { useState } from "react";
import { useTranslation } from "react-i18next";
import { fetchApi } from "../utils/api";

/// Follow / Unfollow / Follow back CTA. Click flips the visible state
/// optimistically, then the API call confirms or reverts:
///   - on success: clear the optimistic override so the prop value (which
///     the parent has updated via onChange) becomes the source of truth
///   - on failure: clear the override AND surface an inline error
///
/// `onChange` receives the freshly-returned `{you_follow, follows_you}`
/// so the parent can update counts and pills without a profile reload.
export default function FollowButton({ userId, youFollow, followsYou, onChange }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  // When non-null, the optimistic override wins over props. Cleared when
  // the request settles (parent's onChange has already moved props by
  // then on success; on failure clearing reverts to the prop value).
  const [optimistic, setOptimistic] = useState(null);
  const { t } = useTranslation("browse");

  const effectiveYouFollow = optimistic?.you_follow ?? youFollow;
  const effectiveFollowsYou = optimistic?.follows_you ?? followsYou;

  async function toggle() {
    if (pending) return;
    setError("");
    const next = !effectiveYouFollow;
    setOptimistic({ you_follow: next, follows_you: effectiveFollowsYou });
    setPending(true);
    try {
      const response = await fetchApi(`/api/users/${userId}/follow`, {
        method: next ? "POST" : "DELETE",
      });
      if (!response.ok) throw new Error(t("profile.followFailed"));
      const data = await response.json();
      if (onChange) onChange(data);
      setOptimistic(null);
    } catch (err) {
      setOptimistic(null);
      setError(err.message || t("profile.followFailed"));
    } finally {
      setPending(false);
    }
  }

  // CTA text:
  //   you follow them                    -> "Following"
  //   they follow you, you don't follow  -> "Follow back"
  //   neither                            -> "Follow"
  let label;
  if (effectiveYouFollow) {
    label = t("profile.following");
  } else if (effectiveFollowsYou) {
    label = t("profile.followBack");
  } else {
    label = t("profile.follow");
  }

  return (
    <span className="follow-button-wrap">
      <button
        type="button"
        className={`follow-button ${effectiveYouFollow ? "is-following" : ""}`}
        onClick={toggle}
        disabled={pending}
        aria-pressed={effectiveYouFollow}
      >
        {label}
      </button>
      {error && (
        <span className="follow-button-error" role="alert">
          {error}
        </span>
      )}
    </span>
  );
}

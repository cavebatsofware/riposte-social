import { useState } from "react";
import { useTranslation } from "react-i18next";
import { fetchApi } from "../utils/api";

/// Follow / Unfollow / Follow back CTA. Optimistic flip on click; reverts
/// and surfaces an inline error on failure. Calls `onChange` with the
/// freshly-returned `{you_follow, follows_you}` so the parent can update
/// counts and pills without a profile reload.
export default function FollowButton({ userId, youFollow, followsYou, onChange }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const { t } = useTranslation("browse");

  async function toggle() {
    if (pending) return;
    setError("");
    setPending(true);
    const next = !youFollow;
    try {
      const response = await fetchApi(`/api/users/${userId}/follow`, {
        method: next ? "POST" : "DELETE",
      });
      if (!response.ok) throw new Error(t("profile.followFailed"));
      const data = await response.json();
      if (onChange) onChange(data);
    } catch (err) {
      setError(err.message || t("profile.followFailed"));
    } finally {
      setPending(false);
    }
  }

  // CTA text:
  //   you follow them, mutual            -> "Following" (hover-to-unfollow handled visually)
  //   you follow them, not mutual        -> "Following"
  //   they follow you, you don't follow  -> "Follow back"
  //   neither                            -> "Follow"
  let label;
  if (youFollow) {
    label = t("profile.following");
  } else if (followsYou) {
    label = t("profile.followBack");
  } else {
    label = t("profile.follow");
  }

  return (
    <span className="follow-button-wrap">
      <button
        type="button"
        className={`follow-button ${youFollow ? "is-following" : ""}`}
        onClick={toggle}
        disabled={pending}
        aria-pressed={youFollow}
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

import { useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import {
  getLoadingSnapshot,
  subscribeLoading,
  type LoadingState,
} from "../utils/loadingState";

/// Thin progress strip fixed to the top of the viewport. Subscribes to
/// the global loading bucket and renders its `progress` / `visible`
/// values directly; all state transitions live in `loadingState`.
export default function LoadingBar() {
  const { t } = useTranslation("common");
  const { progress, visible }: LoadingState = useSyncExternalStore(
    subscribeLoading,
    getLoadingSnapshot,
    getLoadingSnapshot,
  );
  return (
    <div
      className={`loading-bar ${visible ? "is-visible" : ""}`}
      style={{ width: `${progress}%` }}
      role="progressbar"
      aria-label={t("loading")}
      aria-hidden={!visible}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(progress)}
    />
  );
}

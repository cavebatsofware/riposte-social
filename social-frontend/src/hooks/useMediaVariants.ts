import { useEffect, useState } from "react";
import { fetchApi } from "../utils/api";

/// Per-request id cap. Desktop sends 4 (the server's maximum, set by
/// `MEDIA_VARIANTS_BATCH_MAX` in `src/posts/types.rs`); touch-primary
/// devices drop to 1 so the album shows the first item sooner
/// and the payload over a cellular link stays bounded.
/// Detected once at module load via `(pointer: coarse)` rather than
/// per-request; device input class doesn't change without a reload.
const BATCH_SIZE: number =
  typeof window !== "undefined" && window.matchMedia("(pointer: coarse)").matches
    ? 1
    : 4;

type VariantsByMediaId = Record<string, string | null>;

/// Fetch pre-generated thumbnail variants for the given post or album's
/// media ids. Splits the id list into `BATCH_SIZE`-sized chunks and
/// requests them **one at a time** so a connection isn't fragmented
/// across many concurrent requests sharing the same bandwidth cap. Each
/// batch's response merges into state as it arrives, so the grid fills
/// in row-by-row in upload order rather than indeterminately.
///
/// Each batch passes `hasProgress: [batchNumber, totalBatches]`. The loop
/// reads as one unit in the global loading bucket; the bar advances by
/// `1 / totalBatches` per batch.
///
/// Cancels in-flight work on unmount or when inputs change via
/// `AbortController`, so a cancelled batch's `endLoad` closes its bucket
/// unit cleanly without leaving the bar hung on a stale fractional state.
export function useMediaVariants(parentId: string | null, mediaIds: string[]): VariantsByMediaId {
  const [variants, setVariants] = useState<VariantsByMediaId>({});
  const idsKey = mediaIds.join(",");

  useEffect(() => {
    setVariants({});
    if (!parentId || mediaIds.length === 0) return undefined;

    const totalBatches = Math.ceil(mediaIds.length / BATCH_SIZE);
    const controller = new AbortController();

    async function loadBatchesInOrder() {
      for (let i = 0; i < mediaIds.length; i += BATCH_SIZE) {
        if (controller.signal.aborted) return;
        const batch = mediaIds.slice(i, i + BATCH_SIZE);
        const batchNumber = Math.floor(i / BATCH_SIZE) + 1;
        const params = new URLSearchParams({
          variant: "thumbnail",
          ids: batch.join(","),
        });
        try {
          const response = await fetchApi(
            `/api/posts/${parentId}/media-variants?${params.toString()}`,
            { hasProgress: [batchNumber, totalBatches], signal: controller.signal },
          );
          if (controller.signal.aborted || !response.ok) continue;
          const data = (await response.json()) as {
            variants: { id: string; data: string | null }[];
          };
          if (controller.signal.aborted) return;
          setVariants((prev) => {
            const next = { ...prev };
            for (const v of data.variants) next[v.id] = v.data;
            return next;
          });
        } catch {
          // Aborts and network hiccups both land here; on abort the loop
          // exits at the next iteration's signal check. On a real network
          // error the placeholder stays for that batch's items and a
          // subsequent navigation retries.
        }
      }
    }
    loadBatchesInOrder();
    return () => {
      controller.abort();
    };
  }, [parentId, idsKey]);

  return variants;
}

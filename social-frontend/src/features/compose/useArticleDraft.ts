import { useCallback, useEffect, useRef, useState } from "react";
import { fetchApi } from "../../utils/api";
import {
  createArticle,
  fetchArticle,
  updateArticle,
} from "../articles/api";
import type {
  ArticleResponse,
  CreateArticleRequest,
  UpdateArticleRequest,
} from "../../types/api";

export type DraftStatus = "unsaved" | "draft" | "published";

interface UseArticleDraftOptions {
  initialId: string | null;
  userId: string | null;
}

interface PublishOptions {
  visibility: string;
  categoryId: string;
}

interface UploadedMedia {
  id: string;
  url: string;
}

interface DraftFields {
  title: string;
  subtitle: string;
  body: string;
  visibility: string;
  categoryId: string;
}

const EMPTY_FIELDS: DraftFields = {
  title: "",
  subtitle: "",
  body: "",
  visibility: "private",
  categoryId: "",
};

const AUTOSAVE_DELAY_MS = 3000;
// Backoff schedule for autosave retries after a failed flush. After the
// last entry is exhausted the retry loop stops; the user can recover by
// editing (which resets the counter) or clicking Save draft.
const RETRY_DELAYS_MS = [2000, 5000, 15000];
const SHADOW_PREFIX = "articleDraft:";

function shadowKey(userId: string | null): string | null {
  return userId ? `${SHADOW_PREFIX}${userId}` : null;
}

function readShadow(userId: string | null): DraftFields | null {
  const key = shadowKey(userId);
  if (!key || typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return null;
    return {
      title: typeof parsed.title === "string" ? parsed.title : "",
      subtitle: typeof parsed.subtitle === "string" ? parsed.subtitle : "",
      body: typeof parsed.body === "string" ? parsed.body : "",
      visibility:
        typeof parsed.visibility === "string" ? parsed.visibility : "private",
      categoryId:
        typeof parsed.categoryId === "string" ? parsed.categoryId : "",
    };
  } catch {
    return null;
  }
}

function writeShadow(userId: string | null, fields: DraftFields): void {
  const key = shadowKey(userId);
  if (!key || typeof window === "undefined") return;
  try {
    window.localStorage.setItem(key, JSON.stringify(fields));
  } catch {
    // localStorage may be full or disabled; the server is still authoritative.
  }
}

function clearShadow(userId: string | null): void {
  const key = shadowKey(userId);
  if (!key || typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(key);
  } catch {
    // ignore
  }
}

function fieldsFromArticle(data: ArticleResponse): DraftFields {
  return {
    title: data.title || "",
    subtitle: data.subtitle || "",
    body: data.body || "",
    visibility: data.visibility || "private",
    categoryId: data.category ? data.category.id : "",
  };
}

// Canonical patch shape. flushPatch and publish both go through here so
// the wire format stays in one place. Overrides let publish flip
// visibility/category/is_draft without rebuilding the whole object.
function buildUpdatePatch(
  fields: DraftFields,
  overrides: Partial<UpdateArticleRequest> = {},
): UpdateArticleRequest {
  return {
    title: fields.title,
    subtitle: fields.subtitle,
    body: fields.body,
    excerpt: null,
    visibility: fields.visibility,
    category_id: fields.categoryId || null,
    clear_category: !fields.categoryId,
    cover_media_id: null,
    clear_cover: false,
    is_draft: null,
    ...overrides,
  };
}

function buildCreatePayload(
  fields: DraftFields,
  isDraft: boolean,
): CreateArticleRequest {
  return {
    title: fields.title,
    body: fields.body || null,
    subtitle: fields.subtitle || null,
    excerpt: null,
    visibility: isDraft ? null : fields.visibility || null,
    category_id: fields.categoryId || null,
    is_draft: isDraft,
  };
}

async function uploadArticleMedia(
  articleId: string,
  file: File,
): Promise<UploadedMedia> {
  const form = new FormData();
  form.append("media", file);
  const response = await fetchApi(`/api/articles/${articleId}/media`, {
    method: "POST",
    body: form,
  });
  if (!response.ok) {
    throw new Error("upload_failed");
  }
  const items = await response.json();
  if (!Array.isArray(items) || items.length === 0) {
    throw new Error("upload_failed");
  }
  return { id: items[0].id, url: items[0].url };
}

export function useArticleDraft({
  initialId,
  userId,
}: UseArticleDraftOptions) {
  // All five editable fields live in one state object so callbacks only
  // need to track a single ref. Cover state has a separate lifecycle
  // (mutated by upload paths, not by typing) and stays its own state.
  const [fields, setFieldsState] = useState<DraftFields>(EMPTY_FIELDS);
  const fieldsRef = useRef(fields);
  fieldsRef.current = fields;

  const [coverMediaId, setCoverMediaId] = useState<string | null>(null);
  const [coverUrl, setCoverUrl] = useState<string | null>(null);

  const [id, setId] = useState<string | null>(initialId);
  const [status, setStatus] = useState<DraftStatus>(
    initialId ? "draft" : "unsaved",
  );
  const [loading, setLoading] = useState(Boolean(initialId));
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  const idRef = useRef<string | null>(initialId);
  idRef.current = id;
  const statusRef = useRef<DraftStatus>(status);
  statusRef.current = status;

  // Autosave + retry plumbing.
  // - timerRef: shared slot for the debounced autosave OR a backoff retry.
  //   At most one is armed at a time; user activity replaces whichever it is.
  // - retryAttemptRef: index into RETRY_DELAYS_MS. Reset on success or on
  //   any user edit (a fresh edit is not a retry).
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const retryAttemptRef = useRef(0);

  // Every write to this article (autosave, Save draft, Publish) runs through
  // one promise chain so two PATCHes can never overlap and let the network
  // reorder them, clobbering newer fields with older ones. Tasks run FIFO and
  // read fieldsRef when they execute, so each sends the latest state.
  const writeChainRef = useRef<Promise<void>>(Promise.resolve());

  // Bounds the chain to a single queued autosave. While one is queued or in
  // flight (autosavePendingRef), a fresh edit only marks the draft dirty; the
  // in-flight autosave re-fires once on success to capture the trailing edits.
  // Combined with the composer's submitting flag gating Save/Publish, chain
  // depth stays at one autosave plus at most one user-initiated write.
  const autosavePendingRef = useRef(false);
  const autosaveDirtyRef = useRef(false);

  // flushPatch is referenced by scheduleAutosave, by the retry-arm inside
  // flushPatch itself, and by the unmount-cleanup effect. A ref lets the
  // setTimeout callbacks always call the freshest version without a deps
  // dance.
  const flushPatchRef = useRef<() => Promise<void>>(() => Promise.resolve());

  const restoredFromShadowRef = useRef(false);

  useEffect(() => {
    if (initialId || restoredFromShadowRef.current) return;
    const shadow = readShadow(userId);
    if (!shadow) return;
    restoredFromShadowRef.current = true;
    setFieldsState(shadow);
  }, [initialId, userId]);

  useEffect(() => {
    if (!initialId) return;
    let cancelled = false;
    async function load() {
      setLoading(true);
      setLoadError(null);
      try {
        const response = await fetchArticle(initialId);
        if (!response.ok) throw new Error("load_failed");
        const data: ArticleResponse = await response.json();
        if (cancelled) return;
        setId(data.id);
        setFieldsState(fieldsFromArticle(data));
        setCoverMediaId(data.cover_media_id || null);
        setCoverUrl(data.cover_url || null);
        setStatus(data.is_draft ? "draft" : "published");
      } catch {
        if (!cancelled) setLoadError("load_failed");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [initialId]);

  // Serializes a write behind any in-flight write. Chains regardless of the
  // prior outcome so one failed PATCH does not wedge the queue; the stored
  // tail swallows its own result so a rejection never surfaces as unhandled.
  // The caller observes success/failure through the returned promise.
  const runExclusive = useCallback(<T>(task: () => Promise<T>): Promise<T> => {
    const run = writeChainRef.current.then(task, task);
    writeChainRef.current = run.then(
      () => {},
      () => {},
    );
    return run;
  }, []);

  // Low-level PATCH. Throws on a non-ok response so callers can set policy:
  // autosave swallows and retries, Save draft and Publish propagate.
  const sendPatch = useCallback(
    async (articleId: string, patch: UpdateArticleRequest): Promise<void> => {
      const response = await updateArticle(articleId, patch);
      if (!response.ok) throw new Error("save_failed");
    },
    [],
  );

  const flushPatch = useCallback(async (): Promise<void> => {
    const articleId = idRef.current;
    if (!articleId) return;
    if (autosavePendingRef.current) {
      // An autosave is already on the chain; it reads the latest fields when
      // it runs. Mark dirty so it re-fires once after settling instead of
      // stacking a second PATCH.
      autosaveDirtyRef.current = true;
      return;
    }
    autosavePendingRef.current = true;
    autosaveDirtyRef.current = false;
    let succeeded = false;
    try {
      // Build the patch inside the task so a queued autosave sends the latest
      // fields rather than a snapshot taken when it was enqueued.
      await runExclusive(() =>
        sendPatch(articleId, buildUpdatePatch(fieldsRef.current)),
      );
      succeeded = true;
      setSaveError(null);
      retryAttemptRef.current = 0;
    } catch {
      // Autosave swallows the failure and re-arms a backoff retry; saveError
      // surfaces it. A fresh user edit resets the attempt via scheduleAutosave.
      setSaveError("save_failed");
      const attempt = retryAttemptRef.current;
      if (attempt < RETRY_DELAYS_MS.length) {
        retryAttemptRef.current = attempt + 1;
        if (timerRef.current) clearTimeout(timerRef.current);
        timerRef.current = setTimeout(() => {
          timerRef.current = null;
          void flushPatchRef.current();
        }, RETRY_DELAYS_MS[attempt]);
      }
    } finally {
      autosavePendingRef.current = false;
    }
    // Edits arrived while this autosave was in flight: run one coalesced
    // follow-up. On failure the backoff retry already covers them.
    if (succeeded && autosaveDirtyRef.current) {
      autosaveDirtyRef.current = false;
      void flushPatchRef.current();
    }
  }, [runExclusive, sendPatch]);
  flushPatchRef.current = flushPatch;

  const scheduleAutosave = useCallback(() => {
    if (!idRef.current) return;
    // A fresh user edit isn't a retry; start the backoff over.
    retryAttemptRef.current = 0;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      void flushPatchRef.current();
    }, AUTOSAVE_DELAY_MS);
  }, []);

  const setField = useCallback(
    <K extends keyof DraftFields>(key: K, value: DraftFields[K]) => {
      const next = { ...fieldsRef.current, [key]: value };
      fieldsRef.current = next;
      setFieldsState(next);
      if (statusRef.current === "unsaved") {
        writeShadow(userId, next);
      } else {
        scheduleAutosave();
      }
    },
    [userId, scheduleAutosave],
  );

  const setTitle = useCallback((v: string) => setField("title", v), [setField]);
  const setSubtitle = useCallback(
    (v: string) => setField("subtitle", v),
    [setField],
  );
  const setBody = useCallback((v: string) => setField("body", v), [setField]);
  const setVisibility = useCallback(
    (v: string) => setField("visibility", v),
    [setField],
  );
  const setCategoryId = useCallback(
    (v: string) => setField("categoryId", v),
    [setField],
  );

  const requireTitleForImage = useCallback(() => {
    return !idRef.current && fieldsRef.current.title.trim().length === 0;
  }, []);

  // The single createArticle path. Every caller that needs an id (Save draft,
  // Publish, inline/cover uploads) routes through here so two creates can't
  // race into two articles. Running on the write chain with an in-chain id
  // re-check collapses concurrent callers onto one create: the first sets
  // idRef, the rest return it without creating.
  const mintDraft = useCallback((): Promise<string> => {
    if (idRef.current) return Promise.resolve(idRef.current);
    if (fieldsRef.current.title.trim().length === 0) {
      return Promise.reject(new Error("title_required"));
    }
    return runExclusive(async () => {
      if (idRef.current) return idRef.current;
      const response = await createArticle(
        buildCreatePayload(fieldsRef.current, true),
      );
      if (!response.ok) throw new Error("save_failed");
      const data: ArticleResponse = await response.json();
      idRef.current = data.id;
      setId(data.id);
      statusRef.current = "draft";
      setStatus("draft");
      clearShadow(userId);
      if (typeof window !== "undefined") {
        const url = new URL(window.location.href);
        url.searchParams.set("id", data.id);
        window.history.replaceState({}, "", url.toString());
      }
      return data.id;
    });
  }, [runExclusive, userId]);

  const saveDraft = useCallback(async () => {
    if (fieldsRef.current.title.trim().length === 0) {
      throw new Error("title_required");
    }
    // This explicit save supersedes any pending debounce or backoff retry.
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    if (!idRef.current) {
      await mintDraft();
      return;
    }
    const articleId = idRef.current;
    // Same serialized PATCH as autosave, but failures propagate to the button.
    await runExclusive(() =>
      sendPatch(articleId, buildUpdatePatch(fieldsRef.current)),
    );
    setSaveError(null);
    retryAttemptRef.current = 0;
  }, [mintDraft, runExclusive, sendPatch]);

  const publish = useCallback(
    async (opts: PublishOptions): Promise<{ id: string }> => {
      const snap = fieldsRef.current;
      if (snap.title.trim().length === 0) throw new Error("title_required");

      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }

      // For an already-published article, "publish" is just a final flush;
      // status stays "published" and we don't touch is_draft. For a draft
      // being published, flip is_draft to false.
      const isAlreadyPublished =
        statusRef.current === "published" && idRef.current !== null;

      // Ensure a draft exists first, through the single deduped create path,
      // so a concurrent inline/cover upload that also mints can't create a
      // second article. Then PATCH it to published on the same chain so the
      // publish overrides land last and win.
      const articleId = await mintDraft();
      await runExclusive(() =>
        sendPatch(
          articleId,
          buildUpdatePatch(fieldsRef.current, {
            visibility: opts.visibility,
            category_id: opts.categoryId || null,
            clear_category: !opts.categoryId,
            is_draft: isAlreadyPublished ? null : false,
          }),
        ),
      );
      setSaveError(null);
      retryAttemptRef.current = 0;
      if (!isAlreadyPublished) {
        statusRef.current = "published";
        setStatus("published");
      }
      return { id: articleId };
    },
    [mintDraft, runExclusive, sendPatch],
  );

  const uploadInlineImage = useCallback(
    async (file: File): Promise<UploadedMedia> => {
      const draftId = idRef.current ?? (await mintDraft());
      return uploadArticleMedia(draftId, file);
    },
    [mintDraft],
  );

  const uploadCover = useCallback(
    async (file: File): Promise<UploadedMedia> => {
      const draftId = idRef.current ?? (await mintDraft());
      const media = await uploadArticleMedia(draftId, file);
      const response = await updateArticle(draftId, {
        cover_media_id: media.id,
      });
      if (!response.ok) throw new Error("save_failed");
      setCoverMediaId(media.id);
      setCoverUrl(media.url);
      return media;
    },
    [mintDraft],
  );

  const removeCover = useCallback(async () => {
    const draftId = idRef.current;
    if (!draftId) {
      setCoverMediaId(null);
      setCoverUrl(null);
      return;
    }
    const response = await updateArticle(draftId, { clear_cover: true });
    if (!response.ok) throw new Error("save_failed");
    setCoverMediaId(null);
    setCoverUrl(null);
  }, []);

  // Flush any pending autosave on unmount so a quick navigation after
  // typing doesn't drop the last edit. Note that the browser may kill
  // an in-flight fetch on tab close; the retry path covers transient
  // failures while the tab is still open.
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        // A debounce or backoff was still armed: an unflushed edit or a
        // pending retry. Fire one last autosave so a quick navigation after
        // typing doesn't drop the final edit.
        clearTimeout(timerRef.current);
        timerRef.current = null;
        void flushPatchRef.current();
      }
    };
  }, []);

  return {
    id,
    title: fields.title,
    subtitle: fields.subtitle,
    body: fields.body,
    coverMediaId,
    coverUrl,
    visibility: fields.visibility,
    categoryId: fields.categoryId,
    status,
    loading,
    loadError,
    saveError,
    setTitle,
    setSubtitle,
    setBody,
    setVisibility,
    setCategoryId,
    saveDraft,
    publish,
    uploadInlineImage,
    uploadCover,
    removeCover,
    requireTitleForImage,
  };
}

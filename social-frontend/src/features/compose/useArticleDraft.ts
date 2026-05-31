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
  // - inFlightRef: true while a flushPatch is awaiting the server. Prevents
  //   a second PATCH from racing the first, which would let the network
  //   reorder them and overwrite newer edits with older ones.
  // - pendingPatchRef: true when an edit happened during an in-flight flush;
  //   the completing flush re-arms itself to capture the new state.
  // - retryAttemptRef: index into RETRY_DELAYS_MS. Reset on success or on
  //   any user edit (a fresh edit is not a retry).
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlightRef = useRef(false);
  const pendingPatchRef = useRef(false);
  const retryAttemptRef = useRef(0);

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

  const flushPatch = useCallback(async (): Promise<void> => {
    const articleId = idRef.current;
    if (!articleId) return;
    if (inFlightRef.current) {
      // Another flush is mid-flight; mark pending so it re-fires on completion.
      pendingPatchRef.current = true;
      return;
    }
    inFlightRef.current = true;
    pendingPatchRef.current = false;
    const patch = buildUpdatePatch(fieldsRef.current);
    let succeeded = false;
    try {
      const response = await updateArticle(articleId, patch);
      if (!response.ok) throw new Error("save_failed");
      succeeded = true;
    } catch {
      // fall through; recovery is below so inFlightRef is always cleared.
    } finally {
      inFlightRef.current = false;
    }
    if (succeeded) {
      setSaveError(null);
      retryAttemptRef.current = 0;
      // Coalesce edits made during the flush.
      if (pendingPatchRef.current) {
        pendingPatchRef.current = false;
        void flushPatchRef.current();
      }
      return;
    }
    setSaveError("save_failed");
    pendingPatchRef.current = true;
    const attempt = retryAttemptRef.current;
    if (attempt >= RETRY_DELAYS_MS.length) return;
    retryAttemptRef.current = attempt + 1;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      void flushPatchRef.current();
    }, RETRY_DELAYS_MS[attempt]);
  }, []);
  flushPatchRef.current = flushPatch;

  const scheduleAutosave = useCallback(() => {
    if (!idRef.current) return;
    pendingPatchRef.current = true;
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

  const mintDraft = useCallback(async (): Promise<string> => {
    if (idRef.current) return idRef.current;
    const snap = fieldsRef.current;
    if (snap.title.trim().length === 0) throw new Error("title_required");

    const response = await createArticle(buildCreatePayload(snap, true));
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
  }, [userId]);

  const saveDraft = useCallback(async () => {
    if (fieldsRef.current.title.trim().length === 0) {
      throw new Error("title_required");
    }
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    if (!idRef.current) {
      await mintDraft();
      return;
    }
    await flushPatch();
  }, [mintDraft, flushPatch]);

  const publish = useCallback(
    async (opts: PublishOptions): Promise<{ id: string }> => {
      const snap = fieldsRef.current;
      if (snap.title.trim().length === 0) throw new Error("title_required");

      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }

      const articleId = idRef.current;
      // For an already-published article, "publish" is just a final flush;
      // status stays "published" and we don't touch is_draft. For a draft
      // being published, flip is_draft to false.
      const isAlreadyPublished =
        statusRef.current === "published" && articleId !== null;

      if (articleId) {
        const patch = buildUpdatePatch(snap, {
          visibility: opts.visibility,
          category_id: opts.categoryId || null,
          clear_category: !opts.categoryId,
          is_draft: isAlreadyPublished ? null : false,
        });
        const response = await updateArticle(articleId, patch);
        if (!response.ok) throw new Error("save_failed");
        if (!isAlreadyPublished) {
          statusRef.current = "published";
          setStatus("published");
        }
        return { id: articleId };
      }

      // No draft exists yet: create the published article in one shot.
      const payload = buildCreatePayload(
        {
          ...snap,
          visibility: opts.visibility,
          categoryId: opts.categoryId,
        },
        false,
      );
      const response = await createArticle(payload);
      if (!response.ok) throw new Error("save_failed");
      const data: ArticleResponse = await response.json();
      idRef.current = data.id;
      setId(data.id);
      statusRef.current = "published";
      setStatus("published");
      clearShadow(userId);
      return { id: data.id };
    },
    [userId],
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
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      if (pendingPatchRef.current) {
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

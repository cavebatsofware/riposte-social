import { useCallback, useEffect, useRef, useState } from "react";
import { fetchApi } from "../../utils/api";
import {
  createArticle,
  fetchArticle,
  updateArticle,
} from "../articles/api";

export type DraftStatus = "unsaved" | "draft" | "published";

interface UseArticleDraftOptions {
  initialId: string | null;
  userId: string | null;
}

interface Shadow {
  title: string;
  subtitle: string;
  body: string;
  visibility: string;
  categoryId: string;
}

interface PublishOptions {
  visibility: string;
  categoryId: string;
}

interface UploadedMedia {
  id: string;
  url: string;
}

const AUTOSAVE_DELAY_MS = 3000;
const SHADOW_PREFIX = "articleDraft:";

function shadowKey(userId: string | null): string | null {
  return userId ? `${SHADOW_PREFIX}${userId}` : null;
}

function readShadow(userId: string | null): Shadow | null {
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

function writeShadow(userId: string | null, shadow: Shadow): void {
  const key = shadowKey(userId);
  if (!key || typeof window === "undefined") return;
  try {
    window.localStorage.setItem(key, JSON.stringify(shadow));
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
  const [id, setId] = useState<string | null>(initialId);
  const [title, setTitleState] = useState("");
  const [subtitle, setSubtitleState] = useState("");
  const [body, setBodyState] = useState("");
  const [coverMediaId, setCoverMediaId] = useState<string | null>(null);
  const [coverUrl, setCoverUrl] = useState<string | null>(null);
  const [visibility, setVisibilityState] = useState("private");
  const [categoryId, setCategoryIdState] = useState("");
  const [status, setStatus] = useState<DraftStatus>(
    initialId ? "draft" : "unsaved",
  );
  const [loading, setLoading] = useState(Boolean(initialId));
  const [loadError, setLoadError] = useState<string | null>(null);

  // Latest field snapshot for the debounced flush; refs avoid recreating the
  // timer when the user types another character mid-debounce.
  const latestRef = useRef({
    title,
    subtitle,
    body,
    visibility,
    categoryId,
    coverMediaId,
  });
  latestRef.current = {
    title,
    subtitle,
    body,
    visibility,
    categoryId,
    coverMediaId,
  };

  const idRef = useRef<string | null>(initialId);
  idRef.current = id;

  const statusRef = useRef<DraftStatus>(status);
  statusRef.current = status;

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingPatchRef = useRef(false);

  const restoredFromShadowRef = useRef(false);

  // Restore localStorage shadow on first mount when there's no server id.
  useEffect(() => {
    if (initialId || restoredFromShadowRef.current) return;
    const shadow = readShadow(userId);
    if (!shadow) return;
    restoredFromShadowRef.current = true;
    setTitleState(shadow.title);
    setSubtitleState(shadow.subtitle);
    setBodyState(shadow.body);
    setVisibilityState(shadow.visibility);
    setCategoryIdState(shadow.categoryId);
  }, [initialId, userId]);

  // Load existing article when entering with ?id=.
  useEffect(() => {
    if (!initialId) return;
    let cancelled = false;
    async function load() {
      setLoading(true);
      setLoadError(null);
      try {
        const response = await fetchArticle(initialId);
        if (!response.ok) {
          throw new Error("load_failed");
        }
        const data = await response.json();
        if (cancelled) return;
        setId(data.id);
        setTitleState(data.title || "");
        setSubtitleState(data.subtitle || "");
        setBodyState(data.body || "");
        setCoverMediaId(data.cover_media_id || null);
        setCoverUrl(data.cover_url || null);
        setVisibilityState(data.visibility || "private");
        setCategoryIdState(data.category ? data.category.id : "");
        setStatus(data.is_draft ? "draft" : "published");
      } catch {
        if (!cancelled) setLoadError("load_failed");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [initialId]);

  const flushPatch = useCallback(async () => {
    const articleId = idRef.current;
    if (!articleId) return;
    pendingPatchRef.current = false;
    const snap = latestRef.current;
    const patch: Record<string, unknown> = {
      title: snap.title,
      subtitle: snap.subtitle,
      body: snap.body,
      visibility: snap.visibility,
    };
    if (snap.categoryId) {
      patch.category_id = snap.categoryId;
    } else {
      patch.clear_category = true;
    }
    await updateArticle(articleId, patch);
  }, []);

  const scheduleAutosave = useCallback(() => {
    if (!idRef.current) return;
    pendingPatchRef.current = true;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      void flushPatch();
    }, AUTOSAVE_DELAY_MS);
  }, [flushPatch]);

  const onFieldChange = useCallback(() => {
    if (statusRef.current === "unsaved") {
      writeShadow(userId, {
        title: latestRef.current.title,
        subtitle: latestRef.current.subtitle,
        body: latestRef.current.body,
        visibility: latestRef.current.visibility,
        categoryId: latestRef.current.categoryId,
      });
    } else {
      scheduleAutosave();
    }
  }, [userId, scheduleAutosave]);

  const setTitle = useCallback(
    (v: string) => {
      setTitleState(v);
      latestRef.current = { ...latestRef.current, title: v };
      onFieldChange();
    },
    [onFieldChange],
  );
  const setSubtitle = useCallback(
    (v: string) => {
      setSubtitleState(v);
      latestRef.current = { ...latestRef.current, subtitle: v };
      onFieldChange();
    },
    [onFieldChange],
  );
  const setBody = useCallback(
    (v: string) => {
      setBodyState(v);
      latestRef.current = { ...latestRef.current, body: v };
      onFieldChange();
    },
    [onFieldChange],
  );
  const setVisibility = useCallback(
    (v: string) => {
      setVisibilityState(v);
      latestRef.current = { ...latestRef.current, visibility: v };
      onFieldChange();
    },
    [onFieldChange],
  );
  const setCategoryId = useCallback(
    (v: string) => {
      setCategoryIdState(v);
      latestRef.current = { ...latestRef.current, categoryId: v };
      onFieldChange();
    },
    [onFieldChange],
  );

  const requireTitleForImage = useCallback(() => {
    return !idRef.current && latestRef.current.title.trim().length === 0;
  }, []);

  const mintDraft = useCallback(async (): Promise<string> => {
    if (idRef.current) return idRef.current;
    const snap = latestRef.current;
    if (snap.title.trim().length === 0) {
      throw new Error("title_required");
    }
    const payload: Record<string, unknown> = {
      title: snap.title,
      body: snap.body,
      is_draft: true,
    };
    if (snap.subtitle) payload.subtitle = snap.subtitle;
    const response = await createArticle(payload);
    if (!response.ok) {
      throw new Error("save_failed");
    }
    const data = await response.json();
    idRef.current = data.id;
    setId(data.id);
    setStatus("draft");
    statusRef.current = "draft";
    clearShadow(userId);
    if (typeof window !== "undefined") {
      const url = new URL(window.location.href);
      url.searchParams.set("id", data.id);
      window.history.replaceState({}, "", url.toString());
    }
    return data.id;
  }, [userId]);

  const saveDraft = useCallback(async () => {
    if (latestRef.current.title.trim().length === 0) {
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
      if (latestRef.current.title.trim().length === 0) {
        throw new Error("title_required");
      }
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      // For published-article edits, "publish" is just a final flush; status stays "published".
      if (statusRef.current === "published" && idRef.current) {
        const snap = latestRef.current;
        const patch: Record<string, unknown> = {
          title: snap.title,
          subtitle: snap.subtitle,
          body: snap.body,
          visibility: opts.visibility,
        };
        if (opts.categoryId) {
          patch.category_id = opts.categoryId;
        } else {
          patch.clear_category = true;
        }
        const response = await updateArticle(idRef.current, patch);
        if (!response.ok) throw new Error("save_failed");
        return { id: idRef.current };
      }
      const snap = latestRef.current;
      if (!idRef.current) {
        const payload: Record<string, unknown> = {
          title: snap.title,
          subtitle: snap.subtitle || undefined,
          body: snap.body,
          visibility: opts.visibility,
          is_draft: false,
        };
        if (opts.categoryId) payload.category_id = opts.categoryId;
        const response = await createArticle(payload);
        if (!response.ok) throw new Error("save_failed");
        const data = await response.json();
        idRef.current = data.id;
        setId(data.id);
        setStatus("published");
        statusRef.current = "published";
        clearShadow(userId);
        return { id: data.id };
      }
      const patch: Record<string, unknown> = {
        title: snap.title,
        subtitle: snap.subtitle,
        body: snap.body,
        visibility: opts.visibility,
        is_draft: false,
      };
      if (opts.categoryId) {
        patch.category_id = opts.categoryId;
      } else {
        patch.clear_category = true;
      }
      const response = await updateArticle(idRef.current, patch);
      if (!response.ok) throw new Error("save_failed");
      setStatus("published");
      statusRef.current = "published";
      return { id: idRef.current };
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

  // Flush any pending autosave on unmount or page hide so a quick navigation
  // after typing doesn't drop the last edit.
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      if (pendingPatchRef.current) {
        void flushPatch();
      }
    };
  }, [flushPatch]);

  return {
    id,
    title,
    subtitle,
    body,
    coverMediaId,
    coverUrl,
    visibility,
    categoryId,
    status,
    loading,
    loadError,
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

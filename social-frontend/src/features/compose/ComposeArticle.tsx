import { useEffect, useRef, useState } from "react";
import { Link, Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import MDEditor from "@uiw/react-md-editor";
import "@uiw/react-md-editor/markdown-editor.css";
import { useAuth } from "../../contexts/AuthContext";
import { useSiteConfig } from "../../contexts/SiteConfigContext";
import { useTheme } from "../../contexts/ThemeContext";
import VisibilityPicker from "../../components/VisibilityPicker";
import { fetchCategoriesForCompose } from "./api";
import { useArticleDraft } from "./useArticleDraft";

const ACCEPTED_IMAGE_MIME = ["image/jpeg", "image/png", "image/gif", "image/webp"];
const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

export default function ComposeArticle() {
  const { user, loading: authLoading } = useAuth();
  const { config: site } = useSiteConfig();
  const { mode: themeMode } = useTheme();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const initialId = searchParams.get("id");
  const { t } = useTranslation("articles");
  const { t: tCompose } = useTranslation("compose");
  const { t: tCommon } = useTranslation("common");

  const draft = useArticleDraft({
    initialId,
    userId: user ? user.id : null,
  });

  const [categories, setCategories] = useState<{ id: string; name: string }[]>([]);
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const titleInputRef = useRef<HTMLInputElement | null>(null);
  const editorWrapRef = useRef<HTMLDivElement | null>(null);
  const coverInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function loadCategories() {
      try {
        const response = await fetchCategoriesForCompose();
        if (!response.ok) return;
        const data = await response.json();
        if (!cancelled) setCategories(data.categories || []);
      } catch {
        // optional picker; absence is fine
      }
    }
    loadCategories();
    return () => {
      cancelled = true;
    };
  }, []);

  if (authLoading) {
    return <p>{tCommon("loading")}</p>;
  }
  if (!user) {
    return <Navigate to="/login" replace />;
  }
  if (user.role !== "administrator" && user.role !== "poster") {
    return <Navigate to="/" replace />;
  }
  if (
    user.role === "poster" &&
    !(site !== null && site.poster_posting_enabled === true)
  ) {
    return (
      <>
        <Link to="/articles" className="post-back-link">
          {t("view.backToArticles")}
        </Link>
        <section className="feed-empty">
          <p>{tCompose("post.disabledNotice")}</p>
        </section>
      </>
    );
  }

  function focusTitle() {
    titleInputRef.current?.focus();
  }

  function findEditorTextarea(): HTMLTextAreaElement | null {
    return editorWrapRef.current?.querySelector("textarea") ?? null;
  }

  function insertAtCursor(snippet: string) {
    const textarea = findEditorTextarea();
    if (!textarea) {
      draft.setBody(`${draft.body}\n\n${snippet}\n`);
      return;
    }
    const start = textarea.selectionStart ?? draft.body.length;
    const end = textarea.selectionEnd ?? start;
    const next = `${draft.body.slice(0, start)}${snippet}${draft.body.slice(end)}`;
    draft.setBody(next);
    requestAnimationFrame(() => {
      const ta = findEditorTextarea();
      if (!ta) return;
      const caret = start + snippet.length;
      ta.focus();
      ta.setSelectionRange(caret, caret);
    });
  }

  function isAcceptedImage(file: File): boolean {
    if (!ACCEPTED_IMAGE_MIME.includes(file.type)) return false;
    if (file.size > MAX_IMAGE_BYTES) return false;
    return true;
  }

  async function handleInlineImage(file: File) {
    setError("");
    if (!isAcceptedImage(file)) {
      setError(t("compose.uploadFailed"));
      return;
    }
    if (draft.requireTitleForImage()) {
      setError(t("compose.imageNeedsTitle"));
      focusTitle();
      return;
    }
    try {
      const media = await draft.uploadInlineImage(file);
      insertAtCursor(`![](${media.url})`);
    } catch (err) {
      const code = err instanceof Error ? err.message : "upload_failed";
      if (code === "title_required") {
        setError(t("compose.imageNeedsTitle"));
        focusTitle();
      } else {
        setError(t("compose.uploadFailed"));
      }
    }
  }

  function handlePaste(e: React.ClipboardEvent<HTMLDivElement>) {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of items) {
      if (item.kind === "file" && item.type.startsWith("image/")) {
        const file = item.getAsFile();
        if (!file) continue;
        e.preventDefault();
        void handleInlineImage(file);
        return;
      }
    }
  }

  function handleDrop(e: React.DragEvent<HTMLDivElement>) {
    const files = e.dataTransfer?.files;
    if (!files || files.length === 0) return;
    const first = files[0];
    if (!first.type.startsWith("image/")) return;
    e.preventDefault();
    void handleInlineImage(first);
  }

  function handleDragOver(e: React.DragEvent<HTMLDivElement>) {
    if (e.dataTransfer?.types.includes("Files")) {
      e.preventDefault();
    }
  }

  async function handleCoverChange(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    setError("");
    if (!isAcceptedImage(file)) {
      setError(t("compose.uploadFailed"));
      return;
    }
    if (draft.requireTitleForImage()) {
      setError(t("compose.imageNeedsTitle"));
      focusTitle();
      return;
    }
    try {
      await draft.uploadCover(file);
    } catch (err) {
      const code = err instanceof Error ? err.message : "upload_failed";
      if (code === "title_required") {
        setError(t("compose.imageNeedsTitle"));
        focusTitle();
      } else {
        setError(t("compose.uploadFailed"));
      }
    }
  }

  async function handleRemoveCover() {
    setError("");
    try {
      await draft.removeCover();
    } catch {
      setError(t("compose.uploadFailed"));
    }
  }

  async function handleSaveDraft() {
    setError("");
    if (draft.title.trim().length === 0) {
      setError(t("compose.titleRequired"));
      focusTitle();
      return;
    }
    setSubmitting(true);
    try {
      await draft.saveDraft();
    } catch {
      setError(tCompose("post.saveFailed"));
    } finally {
      setSubmitting(false);
    }
  }

  async function handlePublish() {
    setError("");
    if (draft.title.trim().length === 0) {
      setError(t("compose.titleRequired"));
      focusTitle();
      return;
    }
    setSubmitting(true);
    try {
      const { id } = await draft.publish({
        visibility: draft.visibility,
        categoryId: draft.categoryId,
      });
      navigate(`/articles/${id}`, { replace: true });
    } catch {
      setError(tCompose("post.saveFailed"));
      setSubmitting(false);
    }
  }

  if (draft.loading) {
    return <p>{tCompose("post.loadingExisting")}</p>;
  }
  if (draft.loadError) {
    return (
      <>
        <Link to="/articles" className="post-back-link">
          {t("view.backToArticles")}
        </Link>
        <div className="alert alert-error" role="alert">
          {t("compose.loadFailed")}
        </div>
      </>
    );
  }

  const isPublished = draft.status === "published";
  const statusLabel =
    draft.status === "unsaved"
      ? t("compose.status.unsaved")
      : draft.status === "draft"
        ? t("compose.status.draft")
        : t("compose.status.published");

  return (
    <>
      <Link to="/articles" className="post-back-link">
        {t("view.backToArticles")}
      </Link>

      <section
        className="compose-card compose-article"
        aria-busy={submitting}
        aria-labelledby="compose-article-title"
      >
        <div className="compose-article-header">
          <h2 id="compose-article-title" className="compose-title">
            {t("compose.heading")}
          </h2>
          <span
            className={`compose-article-status status-${draft.status}`}
            aria-label={statusLabel}
          >
            {statusLabel}
          </span>
        </div>

        {error && (
          <div className="alert alert-error" role="alert">
            {error}
          </div>
        )}

        <div className="compose-field">
          <label htmlFor="article-title">
            {t("compose.title")}
            <span aria-hidden="true"> *</span>
          </label>
          {/* eslint-disable-next-line a11yinspect/required-element-warning -- rule fires unconditionally; asterisk in label above is the visual indicator */}
          <input
            ref={titleInputRef}
            id="article-title"
            name="title"
            type="text"
            autoComplete="off"
            className="compose-input compose-article-title-input"
            value={draft.title}
            onChange={(e) => draft.setTitle(e.target.value)}
            placeholder={t("compose.titlePlaceholder")}
            maxLength={200}
            required
          />
        </div>

        <div className="compose-field">
          <label htmlFor="article-subtitle">{t("compose.subtitle")}</label>
          <input
            id="article-subtitle"
            name="subtitle"
            type="text"
            autoComplete="off"
            className="compose-input compose-article-subtitle-input"
            value={draft.subtitle}
            onChange={(e) => draft.setSubtitle(e.target.value)}
            placeholder={t("compose.subtitlePlaceholder")}
          />
        </div>

        <div className="compose-field">
          <label>{t("compose.cover")}</label>
          {draft.coverUrl ? (
            <div className="compose-article-cover">
              <img
                src={draft.coverUrl}
                alt=""
                className="compose-article-cover-thumb"
              />
              <div className="compose-article-cover-actions">
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={() => coverInputRef.current?.click()}
                >
                  {t("compose.coverReplace")}
                </button>
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={handleRemoveCover}
                >
                  {t("compose.coverRemove")}
                </button>
              </div>
            </div>
          ) : (
            <button
              type="button"
              className="btn-secondary"
              onClick={() => coverInputRef.current?.click()}
            >
              {t("compose.coverChoose")}
            </button>
          )}
          <input
            ref={coverInputRef}
            type="file"
            name="cover"
            accept={ACCEPTED_IMAGE_MIME.join(",")}
            style={{ display: "none" }}
            onChange={handleCoverChange}
            aria-label={t("compose.cover")}
          />
        </div>

        <div className="compose-field">
          <label htmlFor="article-body">{t("compose.body")}</label>
          <div
            ref={editorWrapRef}
            className="compose-article-editor"
            data-color-mode={themeMode}
            onPaste={handlePaste}
            onDrop={handleDrop}
            onDragOver={handleDragOver}
          >
            <MDEditor
              value={draft.body}
              onChange={(v) => draft.setBody(v ?? "")}
              preview="live"
              height={520}
              visibleDragbar={false}
              textareaProps={{
                id: "article-body",
                placeholder: tCompose("body.placeholder"),
              }}
            />
          </div>
        </div>

        {draft.categoryId ? null : (
          <VisibilityPicker
            value={draft.visibility}
            onChange={draft.setVisibility}
          />
        )}

        <div className="compose-field-row">
          <div className="compose-field">
            <label htmlFor="article-category">{t("compose.category")}</label>
            <select
              id="article-category"
              className="compose-input"
              value={draft.categoryId}
              onChange={(e) => draft.setCategoryId(e.target.value)}
            >
              <option value="">{tCompose("category.uncategorized")}</option>
              {categories.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="compose-footer">
          <span className="form-hint">
            {isPublished
              ? tCompose("post.editHint")
              : tCompose("post.publishHint")}
          </span>
          <div className="compose-footer-actions">
            {!isPublished && (
              <button
                type="button"
                className="btn-secondary"
                onClick={handleSaveDraft}
                disabled={submitting || !draft.title.trim()}
              >
                {t("compose.saveDraft")}
              </button>
            )}
            <button
              type="button"
              className="btn-primary"
              onClick={handlePublish}
              disabled={submitting || !draft.title.trim()}
            >
              {submitting
                ? tCommon("actions.saving")
                : isPublished
                  ? t("compose.update")
                  : t("compose.publish")}
            </button>
          </div>
        </div>
      </section>
    </>
  );
}

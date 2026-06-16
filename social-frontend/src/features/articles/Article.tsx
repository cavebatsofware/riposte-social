import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import DOMPurify from "dompurify";
import { useTranslation } from "react-i18next";
import { useAuth } from "../../contexts/AuthContext";
import CommentThread from "../engagement/CommentThread";
import { SkeletonCard } from "@cavebatsofware/riposte-design-system/components";
import { deleteArticle, fetchArticle } from "./api";

/// Permalink page at `/articles/:id`. Renders the article cover as a
/// hero, the title + subtitle + byline + reading time, then the
/// server-rendered markdown body. Reactions and comments use the same
/// `<CommentThread>` and reaction wiring as posts (articles are posts
/// under the hood).
///
/// Per the issue's "Gotcha to handle" section, articles deliberately do
/// not render the generic attached-media gallery: cover image is hero,
/// inline images come from markdown `![](url)` references in the body.
/// Server pre-renders body to HTML via pulldown-cmark + ammonia; the
/// DOMPurify pass here is a defensive second sanitize (same shape as
/// the existing PostCard body render).
export default function Article() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { user } = useAuth();
  const [article, setArticle] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [deleting, setDeleting] = useState(false);
  const { t } = useTranslation("articles");
  const { t: tCommon } = useTranslation("common");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setError("");
      try {
        const response = await fetchArticle(id);
        if (response.status === 404 || response.status === 401) {
          if (!cancelled) {
            setError(t("view.notFound"));
            setArticle(null);
          }
          return;
        }
        if (!response.ok) {
          throw new Error(t("view.loadFailed"));
        }
        const data = await response.json();
        if (!cancelled) {
          setArticle(data);
        }
      } catch (err) {
        if (!cancelled) setError(err.message);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [id, t]);

  const isAuthorOrAdmin =
    user &&
    article &&
    (user.id === article.author_id || user.role === "administrator");

  async function handleDelete() {
    if (!window.confirm(t("view.deleteConfirm"))) {
      return;
    }
    setDeleting(true);
    setError("");
    try {
      const response = await deleteArticle(id);
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("view.deleteFailed"));
      }
      navigate("/articles", { replace: true });
    } catch (err) {
      setError(err.message);
      setDeleting(false);
    }
  }

  const formattedDate = article?.published_at
    ? new Date(article.published_at).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      })
    : "";

  const safeBodyHtml = article ? DOMPurify.sanitize(article.body_html) : "";

  return (
    <article className="article-view">
      <Link to="/articles" className="post-back-link">
        {t("view.backToArticles")}
      </Link>

      {loading && <SkeletonCard />}

      {!loading && error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}

      {!loading && article && (
        <>
          {article.cover_url && (
            <img
              src={article.cover_url}
              alt=""
              className="article-view-cover"
              loading="eager"
            />
          )}

          <header className="article-view-header">
            <h1 className="article-view-title">
              {article.title}
              {article.is_draft && (
                <span className="article-view-draft-pill" aria-label={t("view.draftPillAria")}>
                  {t("view.draftPill")}
                </span>
              )}
            </h1>
            {article.subtitle && (
              <p className="article-view-subtitle">{article.subtitle}</p>
            )}
            <p className="article-view-byline">
              {article.author_handle ? (
                <Link to={`/u/${article.author_handle}`}>
                  {article.author_display || `@${article.author_handle}`}
                </Link>
              ) : (
                article.author_display
              )}
              {formattedDate && (
                <>
                  <span aria-hidden="true"> · </span>
                  <time dateTime={article.published_at}>{formattedDate}</time>
                </>
              )}
              <span aria-hidden="true"> · </span>
              <span>{t("card.readingTime", { count: article.reading_time_minutes })}</span>
            </p>
          </header>

          <div
            className="article-view-body markdown"
            dangerouslySetInnerHTML={{ __html: safeBodyHtml }}
          />

          {isAuthorOrAdmin && (
            <div className="post-permalink-actions">
              <Link
                to={`/compose-article?id=${article.id}`}
                className="btn-secondary"
              >
                {tCommon("actions.edit")}
              </Link>
              <button
                type="button"
                className="btn-secondary"
                onClick={handleDelete}
                disabled={deleting}
              >
                {deleting ? tCommon("actions.deleting") : tCommon("actions.delete")}
              </button>
            </div>
          )}

          <CommentThread target={{ kind: "post", postId: article.id }} />
        </>
      )}
    </article>
  );
}

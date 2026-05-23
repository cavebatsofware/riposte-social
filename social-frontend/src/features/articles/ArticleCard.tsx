import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";

/// Feed/list preview card for an article. Wider and more horizontal than
/// PostCard so the cover image (when present) can carry visual weight.
/// The cover image, the title, and the explicit "Open article" affordance
/// each link to `/articles/:id`; the card chrome itself isn't a click
/// target (so inner links like the author handle stay activatable).
///
/// Accepts either the embedded `article` preview on a feed `PostResponse`
/// (the feed mixes posts and articles) or a standalone `ArticleSummary`
/// from `/api/articles`. Both shapes carry the same render-relevant
/// fields; this card normalizes them.
export default function ArticleCard({ post, summary }) {
  const { t } = useTranslation("articles");

  // Normalize the two input shapes to a single render shape. The feed
  // path passes the full PostResponse (with `article` embedded); the
  // listing path passes an ArticleSummary directly.
  const data = summary
    ? {
        id: summary.id,
        title: summary.title,
        subtitle: summary.subtitle,
        excerpt: summary.excerpt,
        coverUrl: summary.cover_url,
        readingTime: summary.reading_time_minutes,
        authorHandle: summary.author_handle,
        authorDisplay: summary.author_display,
        commentCount: summary.comment_count,
        reactionCount: summary.reaction_count,
        publishedAt: summary.published_at,
        isDraft: summary.is_draft,
      }
    : {
        id: post.id,
        title: post.article?.title || "",
        subtitle: post.article?.subtitle,
        excerpt: post.article?.excerpt,
        coverUrl: post.article?.cover_url,
        readingTime: post.article?.reading_time_minutes ?? 1,
        authorHandle: post.author_handle,
        authorDisplay: post.author_display,
        commentCount: post.comment_count,
        reactionCount: Object.values(post.reaction_counts || {}).reduce(
          (a, b) => a + Number(b || 0),
          0,
        ),
        publishedAt: post.published_at,
        isDraft: false,
      };

  const link = `/articles/${data.id}`;
  const formattedDate = data.publishedAt
    ? new Date(data.publishedAt).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      })
    : "";

  return (
    <article className="article-card" aria-labelledby={`article-${data.id}-title`}>
      {data.coverUrl && (
        <Link to={link} className="article-card-cover-link" tabIndex={-1}>
          <img
            src={data.coverUrl}
            alt=""
            className="article-card-cover"
            loading="lazy"
          />
        </Link>
      )}
      <div className="article-card-body">
        <div className="article-card-header">
          <h2 id={`article-${data.id}-title`} className="article-card-title">
            <Link to={link}>{data.title}</Link>
            {data.isDraft && (
              <span className="article-card-draft-pill" aria-label={t("view.draftPillAria")}>
                {t("view.draftPill")}
              </span>
            )}
          </h2>
          <Link to={link} className="article-card-open">
            {t("card.openArticle")}
          </Link>
        </div>
        {data.subtitle && (
          <p className="article-card-subtitle">{data.subtitle}</p>
        )}
        <p className="article-card-byline">
          {data.authorHandle ? (
            <Link to={`/u/${data.authorHandle}`} className="article-card-author">
              {data.authorDisplay || `@${data.authorHandle}`}
            </Link>
          ) : (
            <span className="article-card-author">{data.authorDisplay}</span>
          )}
          {formattedDate && !data.isDraft && (
            <>
              <span aria-hidden="true"> · </span>
              <time dateTime={data.publishedAt}>{formattedDate}</time>
            </>
          )}
          <span aria-hidden="true"> · </span>
          <span>
            {t("card.readingTime", { count: data.readingTime })}
          </span>
        </p>
        {data.excerpt && (
          <p className="article-card-excerpt">{data.excerpt}</p>
        )}
        <p className="article-card-stats" aria-label={t("card.statsAria")}>
          <span>{t("card.reactions", { count: data.reactionCount })}</span>
          <span aria-hidden="true"> · </span>
          <span>{t("card.comments", { count: data.commentCount })}</span>
        </p>
      </div>
    </article>
  );
}

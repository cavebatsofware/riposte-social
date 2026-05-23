import { useCallback, useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import ArticleCard from "./ArticleCard";
import SkeletonCard from "../../components/SkeletonCard";
import { fetchArticles } from "./api";

const ARTICLES_LIMIT = 20;

/// Browse page at `/articles`. Lists published articles, optionally
/// filtered by category (slug) from the URL. Per-author filtering happens
/// on the profile page via `/api/users/{id}/articles`, not here. The
/// backend already strips drafts and applies the visibility predicate;
/// this component just renders whatever it returns.
export default function Articles() {
  const [params] = useSearchParams();
  const category = params.get("category");
  const [articles, setArticles] = useState([]);
  const [cursor, setCursor] = useState(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const { t } = useTranslation("articles");
  const { t: tCommon } = useTranslation("common");

  const loadPage = useCallback(
    async (nextCursor) => {
      setLoading(true);
      setError("");
      try {
        const qp = new URLSearchParams();
        qp.set("limit", String(ARTICLES_LIMIT));
        if (nextCursor) qp.set("cursor", nextCursor);
        if (category) qp.set("category", category);
        const response = await fetchArticles(qp.toString());
        if (!response.ok) throw new Error(t("browse.loadFailed"));
        const data = await response.json();
        setArticles((prev) =>
          nextCursor ? [...prev, ...data.articles] : data.articles,
        );
        setCursor(data.next_cursor);
        setHasMore(Boolean(data.next_cursor));
      } catch (err) {
        setError(err.message);
      } finally {
        setLoading(false);
      }
    },
    [category, t],
  );

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setArticles([]);
    setCursor(null);
    setHasMore(false);
    loadPage(null);
  }, [loadPage]);

  return (
    <section className="articles-browse">
      <h1>{t("browse.title")}</h1>

      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}

      {loading && articles.length === 0 && (
        <div aria-busy="true">
          {Array.from({ length: 3 }).map((_, i) => (
            <SkeletonCard key={i} />
          ))}
        </div>
      )}

      {!loading && articles.length === 0 && !error && (
        <p className="feed-empty">{t("browse.empty")}</p>
      )}

      <div className="articles-list">
        {articles.map((a) => (
          <ArticleCard key={a.id} summary={a} />
        ))}
      </div>

      {hasMore && (
        <div className="feed-load-more">
          <button
            type="button"
            className="btn-secondary"
            disabled={loading}
            onClick={() => loadPage(cursor)}
          >
            {loading ? tCommon("loading") : tCommon("loadMore")}
          </button>
        </div>
      )}
    </section>
  );
}

import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { fetchApi } from "../utils/api";
import Layout from "../components/Layout";
import "./Categories.css";

/// Overflow page for the BrowseRail's Categories group. Lists every
/// category in ordinal order; clicking one navigates to the feed
/// filtered by that category. Includes a synthetic "Uncategorized"
/// entry at the top.
export default function Categories() {
  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      try {
        const response = await fetchApi("/api/categories");
        if (!response.ok) throw new Error(t("categories.loadFailed"));
        const data = await response.json();
        if (!cancelled) setCategories(data.categories || []);
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
  }, [t]);

  return (
    <Layout>
      <h1 className="overflow-title">{t("categories.pageTitle")}</h1>

      {loading && <p className="muted">{tCommon("loading")}</p>}

      {error && <div className="alert alert-error">{error}</div>}

      <div className="categories-overflow-list">
        <Link to="/" className="categories-overflow-row">
          <span className="categories-overflow-name">
            {t("categories.allPosts")}
          </span>
        </Link>
        <Link to="/?category=uncategorized" className="categories-overflow-row">
          <span className="categories-overflow-name">
            {t("categories.uncategorized")}
          </span>
        </Link>
        {categories.map((c) => (
          <Link
            key={c.id}
            to={`/?category=${encodeURIComponent(c.slug)}`}
            className="categories-overflow-row"
          >
            {c.color && (
              <span
                className="categories-overflow-swatch"
                style={{ backgroundColor: c.color }}
                aria-hidden="true"
              />
            )}
            <span className="categories-overflow-name">{c.name}</span>
            <span className="categories-overflow-slug">@{c.slug}</span>
          </Link>
        ))}
      </div>
    </Layout>
  );
}

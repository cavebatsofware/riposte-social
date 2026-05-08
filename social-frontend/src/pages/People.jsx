import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { fetchApi } from "../utils/api";
import Layout from "../components/Layout";
import "./People.css";

/// Overflow page for the BrowseRail's People group. Shows every member
/// the caller can see (excluding self) in a grid with avatar + display
/// name + handle. The rail caps at top-10; this page shows everything.
export default function People() {
  const [people, setPeople] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      try {
        const response = await fetchApi("/api/profiles?limit=200");
        if (!response.ok) throw new Error(t("people.loadFailed"));
        const data = await response.json();
        if (!cancelled) setPeople(data.profiles || []);
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
      <h1 className="overflow-title">{t("people.pageTitle")}</h1>

      {loading && <p className="muted">{tCommon("loading")}</p>}

      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}

      {!loading && people.length === 0 && !error && (
        <p className="muted">{t("people.empty")}</p>
      )}

      <div className="people-overflow-grid">
        {people.map((p) => (
          <Link
            key={p.handle}
            to={`/u/${encodeURIComponent(p.handle)}`}
            className="people-overflow-card"
          >
            <div className="people-overflow-avatar">
              {p.avatar_url ? (
                <img src={p.avatar_url} alt="" />
              ) : (
                <span
                  className="people-overflow-avatar-empty"
                  aria-hidden="true"
                />
              )}
            </div>
            <div className="people-overflow-meta">
              <div className="people-overflow-name">
                {p.display_name || p.handle}
              </div>
              <div className="people-overflow-handle">@{p.handle}</div>
            </div>
          </Link>
        ))}
      </div>
    </Layout>
  );
}

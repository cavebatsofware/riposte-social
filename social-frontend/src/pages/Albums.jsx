import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { fetchApi } from "../utils/api";
import Layout from "../components/Layout";
import "./Albums.css";

/// Overflow page for the BrowseRail's Albums group. Shows every album
/// the caller can read in a grid with cover + name + photo count. The
/// rail caps at top-10; this page shows everything.
export default function Albums() {
  const [albums, setAlbums] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      try {
        const response = await fetchApi("/api/albums?limit=100");
        if (!response.ok) throw new Error(t("albums.loadFailed"));
        const data = await response.json();
        if (!cancelled) setAlbums(data.albums || []);
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
      <h1 className="overflow-title">{t("albums.pageTitle")}</h1>

      {loading && <p className="muted">{tCommon("loading")}</p>}

      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}

      {!loading && albums.length === 0 && !error && (
        <p className="muted">{t("albums.empty")}</p>
      )}

      <div className="albums-overflow-grid">
        {albums.map((a) => (
          <Link key={a.id} to={`/album/${a.id}`} className="albums-overflow-card">
            <div className="albums-overflow-cover">
              {a.cover_url ? (
                <img src={a.cover_url} alt="" />
              ) : (
                <span className="albums-overflow-cover-empty" aria-hidden="true" />
              )}
            </div>
            <div className="albums-overflow-meta">
              <div className="albums-overflow-name">{a.name}</div>
              <div className="albums-overflow-count">
                {t("albums.itemCount", { count: a.photo_count })}
              </div>
            </div>
          </Link>
        ))}
      </div>
    </Layout>
  );
}

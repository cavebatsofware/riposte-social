import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import { recordAlbumVisit } from "../utils/browseHistory";
import Layout from "../components/Layout";
import MediaLightbox from "../components/MediaLightbox";
import SkeletonCard from "../components/SkeletonCard";
import VisibilityBadge from "../components/VisibilityBadge";
import "./Album.css";

/// Permalink page at `/album/:id`. Renders the album header (cover/name/
/// description/byline), an editable visibility menu when the viewer is
/// the author or admin, and a grid of media thumbnails. Clicking a thumb
/// opens the MediaLightbox carousel starting at that index.
///
/// Albums never appear in the feed; this page is the discovery surface,
/// reachable from the left rail's Albums group.
export default function Album() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { user } = useAuth();
  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");

  const [album, setAlbum] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [lightboxIndex, setLightboxIndex] = useState(null);
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setError("");
      try {
        const response = await fetchApi(`/api/albums/${id}`);
        if (response.status === 404 || response.status === 401) {
          if (!cancelled) {
            setError(t("album.notFound"));
            setAlbum(null);
          }
          return;
        }
        if (!response.ok) throw new Error(t("album.loadFailed"));
        const data = await response.json();
        if (!cancelled) {
          setAlbum(data);
          recordAlbumVisit(data.id);
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
  }, [id]);

  const isAuthorOrAdmin =
    user && album && (user.id === album.author_id || user.role === "administrator");

  async function handleDelete() {
    if (!window.confirm(t("album.deleteConfirm"))) return;
    setDeleting(true);
    try {
      const response = await fetchApi(`/api/albums/${id}`, { method: "DELETE" });
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("album.deleteFailed"));
      }
      navigate("/", { replace: true });
    } catch (err) {
      setError(err.message);
      setDeleting(false);
    }
  }

  return (
    <Layout>
      <Link to="/" className="post-back-link">
        {tCommon("backToFeed")}
      </Link>

      {loading && <SkeletonCard />}

      {!loading && error && <div className="alert alert-error">{error}</div>}

      {!loading && album && (
        <>
          <AlbumHeader
            album={album}
            isAuthorOrAdmin={isAuthorOrAdmin}
            onDelete={handleDelete}
            deleting={deleting}
          />
          <AlbumMediaGrid
            media={album.media}
            onOpen={(idx) => setLightboxIndex(idx)}
          />
        </>
      )}

      {lightboxIndex !== null && album && (
        <MediaLightbox
          items={album.media}
          index={lightboxIndex}
          onIndex={setLightboxIndex}
          onClose={() => setLightboxIndex(null)}
        />
      )}
    </Layout>
  );
}

function AlbumHeader({ album, isAuthorOrAdmin, onDelete, deleting }) {
  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");
  const author =
    album.author_display || album.author_handle || t("album.fallbackAuthor");
  return (
    <header className="album-header">
      <h1 className="album-title">{album.name}</h1>
      <div className="album-meta">
        {album.author_handle ? (
          <Link to={`/u/${album.author_handle}`} className="album-meta-author">
            {author}
          </Link>
        ) : (
          <span className="album-meta-author">{author}</span>
        )}
        <span className="album-meta-dot" aria-hidden="true" />
        <span>{t("albums.itemCount", { count: album.photo_count })}</span>
        <span className="album-meta-dot" aria-hidden="true" />
        <VisibilityBadge
          visibility={album.effective_visibility || album.visibility}
          fromCategory={Boolean(album.category_id)}
        />
      </div>
      {album.description && (
        <p className="album-description">{album.description}</p>
      )}
      {isAuthorOrAdmin && (
        <div className="album-actions">
          <Link to={`/compose-album?edit=${album.id}`} className="btn-secondary">
            {t("album.edit")}
          </Link>
          <button
            type="button"
            className="btn-secondary"
            onClick={onDelete}
            disabled={deleting}
          >
            {deleting ? tCommon("actions.deleting") : tCommon("actions.delete")}
          </button>
        </div>
      )}
    </header>
  );
}

function AlbumMediaGrid({ media, onOpen }) {
  const { t } = useTranslation("browse");
  if (!media || media.length === 0) {
    return <p className="muted">{t("album.noItems")}</p>;
  }
  return (
    <div className="album-grid">
      {media.map((m, idx) => (
        <button
          key={m.id}
          type="button"
          className="album-grid-item"
          onClick={() => onOpen(idx)}
          aria-label={m.caption || t("album.itemAria", { index: idx + 1 })}
        >
          {m.media_kind === "video" ? (
            <>
              <video src={m.url} muted playsInline preload="metadata" />
              <span className="album-grid-video-badge" aria-hidden="true">
                ▶
              </span>
            </>
          ) : (
            <img src={m.url} alt={m.caption || ""} />
          )}
          {m.caption && (
            <span className="album-grid-caption">{m.caption}</span>
          )}
        </button>
      ))}
    </div>
  );
}

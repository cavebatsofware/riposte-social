import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import Layout from "../components/Layout";
import MediaLightbox from "../components/MediaLightbox";
import SkeletonCard from "../components/SkeletonCard";
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
            setError("Album not found.");
            setAlbum(null);
          }
          return;
        }
        if (!response.ok) throw new Error("Failed to load album");
        const data = await response.json();
        if (!cancelled) setAlbum(data);
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
    if (!window.confirm("Delete this album? This can't be undone.")) return;
    setDeleting(true);
    try {
      const response = await fetchApi(`/api/albums/${id}`, { method: "DELETE" });
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to delete album");
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
        ← Back to feed
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
  const author = album.author_display || album.author_handle || "Member";
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
        <span>
          {album.photo_count} item{album.photo_count === 1 ? "" : "s"}
        </span>
        <span className="album-meta-dot" aria-hidden="true" />
        <VisibilityChip visibility={album.visibility} />
      </div>
      {album.description && (
        <p className="album-description">{album.description}</p>
      )}
      {isAuthorOrAdmin && (
        <div className="album-actions">
          <Link to={`/compose-album?edit=${album.id}`} className="btn-secondary">
            Edit album
          </Link>
          <button
            type="button"
            className="btn-secondary"
            onClick={onDelete}
            disabled={deleting}
          >
            {deleting ? "Deleting…" : "Delete"}
          </button>
        </div>
      )}
    </header>
  );
}

function AlbumMediaGrid({ media, onOpen }) {
  if (!media || media.length === 0) {
    return <p className="muted">This album has no items yet.</p>;
  }
  return (
    <div className="album-grid">
      {media.map((m, idx) => (
        <button
          key={m.id}
          type="button"
          className="album-grid-item"
          onClick={() => onOpen(idx)}
          aria-label={m.caption || `Item ${idx + 1}`}
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

function VisibilityChip({ visibility }) {
  const cls = `visibility-badge ${visibility}`;
  const label =
    visibility === "private"
      ? "Private"
      : visibility === "commenters"
        ? "Commenters"
        : visibility === "posters"
          ? "Posters"
          : "Public";
  return <span className={cls}>{label}</span>;
}

import { useEffect, useState } from "react";
import { Link, NavLink, useLocation } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../../contexts/AuthContext";
import { fetchProfilesDirectory, fetchUserConnections } from "./api";
import Layout from "../../components/Layout";
import "./People.css";

/// Three tabs:
///   /people             -> Discover (every member the caller can see)
///   /people/following   -> users the viewer follows
///   /people/followers   -> users following the viewer
///
/// All three render a uniform people-grid; the data source switches per
/// tab. The viewer-graph tabs paginate through the follows API; Discover
/// uses the existing `/api/profiles` listing.
export default function People() {
  const { user: viewer } = useAuth();
  const location = useLocation();
  const tab = tabFromPath(location.pathname);
  const { t } = useTranslation("browse");
  const { t: tCommon } = useTranslation("common");

  return (
    <Layout>
      <h1 className="overflow-title">{t("people.pageTitle")}</h1>

      <nav className="people-tabs" aria-label={t("people.pageTitle")}>
        <NavLink
          to="/people"
          end
          className={({ isActive }) =>
            `people-tab ${isActive ? "is-active" : ""}`
          }
        >
          {t("people.tabs.discover")}
        </NavLink>
        <NavLink
          to="/people/following"
          className={({ isActive }) =>
            `people-tab ${isActive ? "is-active" : ""}`
          }
        >
          {t("people.tabs.following")}
        </NavLink>
        <NavLink
          to="/people/followers"
          className={({ isActive }) =>
            `people-tab ${isActive ? "is-active" : ""}`
          }
        >
          {t("people.tabs.followers")}
        </NavLink>
      </nav>

      {tab === "discover" && <DiscoverTab t={t} tCommon={tCommon} />}
      {tab === "following" && (
        <GraphTab
          viewerId={viewer?.id}
          variant="following"
          t={t}
          tCommon={tCommon}
        />
      )}
      {tab === "followers" && (
        <GraphTab
          viewerId={viewer?.id}
          variant="followers"
          t={t}
          tCommon={tCommon}
        />
      )}
    </Layout>
  );
}

function tabFromPath(pathname) {
  if (pathname.endsWith("/following")) return "following";
  if (pathname.endsWith("/followers")) return "followers";
  return "discover";
}

function DiscoverTab({ t, tCommon }) {
  const [people, setPeople] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setError("");
      try {
        const response = await fetchProfilesDirectory();
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
    <PeopleList
      people={people}
      loading={loading}
      error={error}
      emptyText={t("people.empty")}
      tCommon={tCommon}
    />
  );
}

/// Graph tabs (Following/Followers) both paginate through the follows API
/// against the viewer's own user_id. When `viewerId` is missing (anonymous
/// viewer or auth still resolving) the tab renders a sign-in prompt
/// instead of leaving the spinner stuck on.
function GraphTab({ viewerId, variant, t, tCommon }) {
  const [people, setPeople] = useState([]);
  const [cursor, setCursor] = useState(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!viewerId) {
      setLoading(false);
      setPeople([]);
      setCursor(null);
      setHasMore(false);
      return undefined;
    }
    let cancelled = false;
    async function load() {
      setLoading(true);
      setError("");
      try {
        const response = await fetchUserConnections(viewerId, variant);
        if (!response.ok) throw new Error(t("people.loadFailed"));
        const data = await response.json();
        if (cancelled) return;
        setPeople(toProfiles(data.users));
        setCursor(data.next_cursor);
        setHasMore(Boolean(data.next_cursor));
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
  }, [viewerId, variant, t]);

  if (!viewerId) {
    return (
      <p className="muted">
        <Link to="/login">{t("people.signInPrompt")}</Link>
      </p>
    );
  }

  async function loadMore() {
    if (!cursor) return;
    setLoading(true);
    try {
      const params = new URLSearchParams();
      params.set("limit", "50");
      params.set("cursor", cursor);
      const response = await fetchUserConnections(
        viewerId,
        variant,
        params.toString(),
      );
      if (!response.ok) throw new Error(t("people.loadFailed"));
      const data = await response.json();
      setPeople((prev) => [...prev, ...toProfiles(data.users)]);
      setCursor(data.next_cursor);
      setHasMore(Boolean(data.next_cursor));
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  const emptyText =
    variant === "following"
      ? t("people.yourFollowingEmpty")
      : t("people.yourFollowersEmpty");

  return (
    <>
      <PeopleList
        people={people}
        loading={loading}
        error={error}
        emptyText={emptyText}
        tCommon={tCommon}
      />
      {hasMore && (
        <div className="feed-load-more">
          <button
            type="button"
            className="btn-secondary"
            disabled={loading}
            onClick={loadMore}
          >
            {loading ? tCommon("loading") : tCommon("loadMore")}
          </button>
        </div>
      )}
    </>
  );
}

/// `/api/users/{id}/{followers,following}` returns `UserSummary` rows
/// shaped slightly differently than `/api/profiles`. Normalize them to
/// the same `{handle, display_name, avatar_url}` shape that PeopleList
/// renders.
function toProfiles(users) {
  return (users || []).map((u) => ({
    user_id: u.user_id,
    handle: u.handle,
    display_name: u.display_name,
    avatar_url: u.avatar_url,
    role: u.role,
  }));
}

function PeopleList({ people, loading, error, emptyText, tCommon }) {
  return (
    <>
      {loading && people.length === 0 && (
        <p className="muted">{tCommon("loading")}</p>
      )}
      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}
      {!loading && people.length === 0 && !error && (
        <p className="muted">{emptyText}</p>
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
    </>
  );
}

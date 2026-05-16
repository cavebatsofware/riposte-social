import { useEffect, useId, useMemo, useState } from "react";
import { Link, useLocation, useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../contexts/AuthContext";
import { fetchAlbums } from "../features/albums/api";
import { fetchCategories } from "../features/categories/api";
import { fetchProfilesDirectory } from "../features/profile/api";
import { readBrowseHistory } from "../utils/browseHistory";
import "./BrowseRail.css";

const TOP_N = 10;
const GROUP_OPEN_KEY = "rs_browse_groups_v1";

/// Two-group accordion (Categories + Albums) rendered in `<Layout>`'s
/// leftRail slot. Each group has the same internal structure:
///   1. search input (filters the full visible list)
///   2. "Show all" link (overflow page)
///   3. up to TOP_N items: MRU intersected with visible set, padded by
///      most-recently-published items so day-1 use isn't sparse.
///
/// Open/closed state per group persists in localStorage. The active
/// category is highlighted from the URL's `?category=<slug>` param.
export default function BrowseRail() {
  const { user } = useAuth();
  const [categories, setCategories] = useState([]);
  const [albums, setAlbums] = useState([]);
  const [people, setPeople] = useState([]);
  const [searchCats, setSearchCats] = useState("");
  const [searchAlbums, setSearchAlbums] = useState("");
  const [searchPeople, setSearchPeople] = useState("");
  const [openGroups, setOpenGroups] = useState(loadOpenGroups());
  const [mru, setMru] = useState({ categories: [], albums: [], people: [] });
  const location = useLocation();
  const [params] = useSearchParams();
  const activeCategory = params.get("category");
  const activeQuery = params.get("q");
  // The category-active styling and aria-current claim only apply when
  // the user is actually on the feed route AND there is no search
  // query. BrowseRail is rendered on many other routes (`/categories`,
  // `/albums`, `/login`...), and clicking the rail links navigates to
  // URLs without `q`, so a feed view filtered by `q` is a different
  // view than what any rail link represents.
  const onFeedRoute = location.pathname === "/" && !activeQuery;
  const { t } = useTranslation("browse");

  // Persist open-state on every change.
  useEffect(() => {
    try {
      localStorage.setItem(GROUP_OPEN_KEY, JSON.stringify(openGroups));
    } catch {
      // ignore — affects only the persistence side
    }
  }, [openGroups]);

  // Re-read MRU whenever the route changes (a category click or album
  // visit will have updated localStorage just before navigation).
  useEffect(() => {
    setMru(readBrowseHistory());
  }, [location.pathname, location.search]);

  // Load category + album lists. Re-fetch when auth state changes (visibility
  // filters apply server-side, so the visible set may grow/shrink on login).
  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const cats = await fetchCategories();
        if (cats.ok) {
          const data = await cats.json();
          if (!cancelled) setCategories(data.categories || []);
        }
      } catch {
        // ignore — rail just hides the empty group
      }
      // Albums require an authed caller in the common case (private
      // by default); anonymous gets a curated list of public-tier
      // albums from the server.
      try {
        const al = await fetchAlbums("limit=100");
        if (al.ok) {
          const data = await al.json();
          if (!cancelled) setAlbums(data.albums || []);
        }
      } catch {
        // ignore
      }
      try {
        const pp = await fetchProfilesDirectory(100);
        if (pp.ok) {
          const data = await pp.json();
          if (!cancelled) setPeople(data.profiles || []);
        }
      } catch {
        // ignore
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [user]);

  function toggleGroup(name) {
    setOpenGroups((prev) => ({ ...prev, [name]: !prev[name] }));
  }

  // Top-N for categories: MRU first (intersected with visible), padded
  // by the unsorted list (which is ordinal-asc from the server) up to
  // TOP_N. Search overrides the top-N view with a substring match across
  // the whole list.
  const categoriesView = useMemo(() => {
    const q = searchCats.trim().toLowerCase();
    if (q) {
      return categories
        .filter(
          (c) =>
            c.name.toLowerCase().includes(q) ||
            c.slug.toLowerCase().includes(q),
        )
        .slice(0, 50);
    }
    return mergeMruWithFallback(
      mru.categories.map((e) => e.slug),
      categories,
      (c) => c.slug,
    );
  }, [searchCats, categories, mru.categories]);

  const albumsView = useMemo(() => {
    const q = searchAlbums.trim().toLowerCase();
    if (q) {
      return albums
        .filter((a) => a.name.toLowerCase().includes(q))
        .slice(0, 50);
    }
    return mergeMruWithFallback(
      mru.albums.map((e) => e.id),
      albums,
      (a) => a.id,
    );
  }, [searchAlbums, albums, mru.albums]);

  const peopleView = useMemo(() => {
    const q = searchPeople.trim().toLowerCase();
    if (q) {
      return people
        .filter(
          (p) =>
            p.handle.toLowerCase().includes(q) ||
            (p.display_name || "").toLowerCase().includes(q),
        )
        .slice(0, 50);
    }
    return mergeMruWithFallback(
      mru.people.map((e) => e.handle),
      people,
      (p) => p.handle,
    );
  }, [searchPeople, people, mru.people]);

  return (
    <nav className="browse-rail" aria-label={t("rail.browseAria")}>
      <Group
        label={t("rail.categoriesGroup")}
        open={openGroups.categories}
        onToggle={() => toggleGroup("categories")}
      >
        <RailSearch
          placeholder={t("rail.searchCategoriesPlaceholder")}
          value={searchCats}
          onChange={setSearchCats}
        />
        <Link to="/categories" className="browse-rail-link browse-rail-overflow">
          {t("rail.showAllCategories")}
        </Link>
        {categoriesView.length === 0 && (
          <p className="browse-rail-empty">{t("rail.noCategories")}</p>
        )}
        {/* Clear-filter affordance. Active when no `?category` is set;
            clicking it always navigates back to the unfiltered feed.
            Sits above the search/show-all controls so it's the
            first/default option a user reaches for. */}
        <Link
          to="/"
          className={`browse-rail-link browse-rail-clear ${
            onFeedRoute && !activeCategory ? "active" : ""
          }`}
          aria-current={onFeedRoute && !activeCategory ? "page" : undefined}
        >
          <span
            className="browse-rail-swatch"
            style={{ backgroundColor: "black" }}
            aria-hidden="true"
          />
          <span className="browse-rail-label">{t("rail.allPosts")}</span>
        </Link>
        {categoriesView.map((c) => {
          const isActive = onFeedRoute && activeCategory === c.slug;
          return (
            <Link
              key={c.id}
              to={`/?category=${encodeURIComponent(c.slug)}`}
              className={`browse-rail-link ${isActive ? "active" : ""}`}
              aria-current={isActive ? "page" : undefined}
            >
              {c.color && (
                <span
                  className="browse-rail-swatch"
                  style={{ backgroundColor: c.color }}
                  aria-hidden="true"
                />
              )}
              <span className="browse-rail-label">{c.name}</span>
            </Link>
          );
        })}
      </Group>

      <Group
        label={t("rail.albumsGroup")}
        open={openGroups.albums}
        onToggle={() => toggleGroup("albums")}
      >
        <RailSearch
          placeholder={t("rail.searchAlbumsPlaceholder")}
          value={searchAlbums}
          onChange={setSearchAlbums}
        />
        <Link to="/albums" className="browse-rail-link browse-rail-overflow">
          {t("rail.showAllAlbums")}
        </Link>
        {albumsView.length === 0 && (
          <p className="browse-rail-empty">{t("rail.noAlbums")}</p>
        )}
        {albumsView.map((a) => (
          <Link
            key={a.id}
            to={`/album/${a.id}`}
            className="browse-rail-link"
            title={a.name}
          >
            {a.cover_url ? (
              <img
                className="browse-rail-cover"
                src={a.cover_url}
                alt=""
                aria-hidden="true"
              />
            ) : (
              <span className="browse-rail-cover browse-rail-cover-empty" aria-hidden="true" />
            )}
            <span className="browse-rail-label">{a.name}</span>
          </Link>
        ))}
      </Group>

      <Group
        label={t("rail.peopleGroup")}
        open={openGroups.people}
        onToggle={() => toggleGroup("people")}
      >
        <RailSearch
          placeholder={t("rail.searchPeoplePlaceholder")}
          value={searchPeople}
          onChange={setSearchPeople}
        />
        <Link to="/people" className="browse-rail-link browse-rail-overflow">
          {t("rail.showAllPeople")}
        </Link>
        {peopleView.length === 0 && (
          <p className="browse-rail-empty">{t("rail.noPeople")}</p>
        )}
        {peopleView.map((p) => (
          <Link
            key={p.handle}
            to={`/u/${encodeURIComponent(p.handle)}`}
            className="browse-rail-link"
            title={p.display_name || p.handle}
          >
            {p.avatar_url ? (
              <img
                className="browse-rail-cover"
                src={p.avatar_url}
                alt=""
                aria-hidden="true"
              />
            ) : (
              <span
                className="browse-rail-cover browse-rail-cover-empty"
                aria-hidden="true"
              />
            )}
            <span className="browse-rail-label">
              {p.display_name || `@${p.handle}`}
            </span>
          </Link>
        ))}
      </Group>
    </nav>
  );
}

function Group({ label, open, onToggle, children }) {
  const bodyId = useId();
  return (
    <section className="browse-rail-group" data-open={open ? "true" : "false"}>
      <button
        type="button"
        className="browse-rail-group-header"
        onClick={onToggle}
        aria-expanded={open}
        aria-controls={bodyId}
      >
        <span className="browse-rail-chevron" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <span>{label}</span>
      </button>
      {open && (
        <div id={bodyId} className="browse-rail-group-body">
          {children}
        </div>
      )}
    </section>
  );
}

function RailSearch({ placeholder, value, onChange }) {
  return (
    <input
      type="search"
      className="browse-rail-search"
      placeholder={placeholder}
      aria-label={placeholder}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

/// Merge an MRU id-list with a full list, preserving MRU order and
/// padding with the full list's order until `TOP_N`. Items not present
/// in `full` (e.g. recently-visited items the viewer no longer has
/// access to) are silently dropped.
function mergeMruWithFallback(mruIds, full, keyOf) {
  const byKey = new Map(full.map((item) => [keyOf(item), item]));
  const out = [];
  const seen = new Set();
  for (const id of mruIds) {
    const item = byKey.get(id);
    if (item && !seen.has(id)) {
      out.push(item);
      seen.add(id);
      if (out.length >= TOP_N) return out;
    }
  }
  for (const item of full) {
    const id = keyOf(item);
    if (!seen.has(id)) {
      out.push(item);
      seen.add(id);
      if (out.length >= TOP_N) return out;
    }
  }
  return out;
}

function loadOpenGroups() {
  try {
    const raw = localStorage.getItem(GROUP_OPEN_KEY);
    if (!raw) return { categories: true, albums: true, people: true };
    const parsed = JSON.parse(raw);
    return {
      categories: parsed.categories !== false,
      albums: parsed.albums !== false,
      people: parsed.people !== false,
    };
  } catch {
    return { categories: true, albums: true, people: true };
  }
}

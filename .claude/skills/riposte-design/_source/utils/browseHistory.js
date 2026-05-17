/// localStorage-backed MRU tracker for the BrowseRail's groups
/// (categories + albums + people). The rail's "top 10" list shows whichever
/// items in the current visible-to-caller set the user has navigated to
/// most recently; this util is the read/write API for that history.
///
/// Storage layout (key: `rs_browse_recent_v1`):
///   {
///     categories: [{ slug: "family",  ts: 1714512000000 }, ...],
///     albums:     [{ id: "uuid",      ts: 1714512000000 }, ...],
///     people:     [{ handle: "grant", ts: 1714512000000 }, ...],
///   }
///
/// Each list is FIFO-capped at 50 entries to bound the storage footprint.
/// Reads return entries in MRU order (most recent first). Writes do an
/// upsert: if the key is present, its `ts` is bumped to `Date.now()` and
/// the entry moves to the front; if absent, it's prepended and the list
/// is truncated.

const STORAGE_KEY = "rs_browse_recent_v1";
const MAX_PER_GROUP = 50;

function loadRaw() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { categories: [], albums: [], people: [] };
    const parsed = JSON.parse(raw);
    return {
      categories: Array.isArray(parsed.categories) ? parsed.categories : [],
      albums: Array.isArray(parsed.albums) ? parsed.albums : [],
      people: Array.isArray(parsed.people) ? parsed.people : [],
    };
  } catch {
    return { categories: [], albums: [], people: [] };
  }
}

function saveRaw(state) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Quota exceeded or private-mode storage; rail still works without
    // MRU  silently swallow.
  }
}

/// Returns the current MRU lists in MRU order (most recent first).
export function readBrowseHistory() {
  const raw = loadRaw();
  return {
    categories: [...raw.categories].sort((a, b) => (b.ts || 0) - (a.ts || 0)),
    albums: [...raw.albums].sort((a, b) => (b.ts || 0) - (a.ts || 0)),
    people: [...raw.people].sort((a, b) => (b.ts || 0) - (a.ts || 0)),
  };
}

/// Record that the viewer just navigated to a category (by slug).
export function recordCategoryVisit(slug) {
  if (!slug) return;
  const state = loadRaw();
  const filtered = state.categories.filter((e) => e.slug !== slug);
  state.categories = [{ slug, ts: Date.now() }, ...filtered].slice(
    0,
    MAX_PER_GROUP,
  );
  saveRaw(state);
}

/// Record that the viewer just navigated to an album (by id).
export function recordAlbumVisit(id) {
  if (!id) return;
  const state = loadRaw();
  const filtered = state.albums.filter((e) => e.id !== id);
  state.albums = [{ id, ts: Date.now() }, ...filtered].slice(0, MAX_PER_GROUP);
  saveRaw(state);
}

/// Record that the viewer just navigated to another member's profile.
export function recordPersonVisit(handle) {
  if (!handle) return;
  const state = loadRaw();
  const filtered = state.people.filter((e) => e.handle !== handle);
  state.people = [{ handle, ts: Date.now() }, ...filtered].slice(
    0,
    MAX_PER_GROUP,
  );
  saveRaw(state);
}

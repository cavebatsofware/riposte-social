/* global React */
const { useState: useStateBR } = React;

/// Three-group accordion (Categories / Albums / People) rendered in the
/// left rail. Each group has a search input, "Show all" link, and a list
/// of items. Open/closed state is local for the demo.
function BrowseRail({ activeCategory, onCategory }) {
  const [open, setOpen] = useStateBR({ categories: true, albums: true, people: false });
  const toggle = (k) => setOpen((p) => ({ ...p, [k]: !p[k] }));

  const categories = [
    { id: "family", name: "Family", color: "var(--color-primary)" },
    { id: "trip-2025", name: "Trip 2025", color: "#8e4a1f" },
    { id: "garden", name: "Garden", color: "#5b1f4d" },
    { id: "kids-art", name: "Kids' art", color: "#1d3557" },
  ];
  const albums = [
    { id: "profile-pictures", name: "Profile pictures" },
    { id: "mobile-uploads", name: "Mobile uploads" },
    { id: "birthday-weekend", name: "Birthday weekend" },
  ];
  const people = [
    { handle: "bob", name: "Bob Bartholomew" },
    { handle: "eve", name: "Eve Erickson" },
    { handle: "carol", name: "Carol Chen" },
  ];

  return (
    <nav className="browse-rail" aria-label="Browse">

      <section className="browse-rail-group">
        <button className="browse-rail-group-header" onClick={() => toggle("categories")} aria-expanded={open.categories}>
          <span className="browse-rail-chevron">{open.categories ? "▾" : "▸"}</span>
          <span>Categories</span>
        </button>
        {open.categories && (
          <div className="browse-rail-group-body">
            <input className="browse-rail-search" type="search" placeholder="Search categories…" aria-label="Search categories" />
            <a className="browse-rail-overflow" href="#">Show all categories →</a>
            <button
              className={`browse-rail-link ${!activeCategory ? "active" : ""}`}
              onClick={() => onCategory(null)}
            >
              <span className="browse-rail-swatch" style={{ background: "black" }} aria-hidden="true" />
              <span className="browse-rail-label">All posts</span>
            </button>
            {categories.map((c) => (
              <button
                key={c.id}
                className={`browse-rail-link ${activeCategory === c.id ? "active" : ""}`}
                onClick={() => onCategory(c.id)}
              >
                <span className="browse-rail-swatch" style={{ background: c.color }} aria-hidden="true" />
                <span className="browse-rail-label">{c.name}</span>
              </button>
            ))}
          </div>
        )}
      </section>

      <section className="browse-rail-group">
        <button className="browse-rail-group-header" onClick={() => toggle("albums")} aria-expanded={open.albums}>
          <span className="browse-rail-chevron">{open.albums ? "▾" : "▸"}</span>
          <span>Albums</span>
        </button>
        {open.albums && (
          <div className="browse-rail-group-body">
            <input className="browse-rail-search" type="search" placeholder="Search albums…" aria-label="Search albums" />
            <a className="browse-rail-overflow" href="#">Show all albums →</a>
            {albums.map((a) => (
              <button key={a.id} className="browse-rail-link">
                <span className="browse-rail-cover" aria-hidden="true" />
                <span className="browse-rail-label">{a.name}</span>
              </button>
            ))}
          </div>
        )}
      </section>

      <section className="browse-rail-group">
        <button className="browse-rail-group-header" onClick={() => toggle("people")} aria-expanded={open.people}>
          <span className="browse-rail-chevron">{open.people ? "▾" : "▸"}</span>
          <span>People</span>
        </button>
        {open.people && (
          <div className="browse-rail-group-body">
            <input className="browse-rail-search" type="search" placeholder="Search people…" aria-label="Search people" />
            <a className="browse-rail-overflow" href="#">Show all people →</a>
            {people.map((p) => (
              <button key={p.handle} className="browse-rail-link">
                <span className="browse-rail-cover" aria-hidden="true" />
                <span className="browse-rail-label">{p.name}</span>
              </button>
            ))}
          </div>
        )}
      </section>

    </nav>
  );
}

Object.assign(window, { BrowseRail });

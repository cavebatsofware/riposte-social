/* global React, ReactDOM, Header, BrowseRail, PostCard, Composer, Login, InviteSplash, Lightbox */
const { useState, useMemo } = React;

/// Sample feed data — shape mirrors the backend's `GET /api/feed` response.
const SAMPLE_POSTS = [
  {
    id: "1",
    author: "Grant DeFayette",
    handle: "grant",
    initials: "GD",
    time: "2 days ago",
    visibility: "commenters",
    bodyHtml: "<p>Got the new chicken run finished this weekend. The hens immediately staged a coup and laid claim to the comfiest corner. Photos below.</p>",
    media: [
      { placeholder: true, label: "Run · wide shot" },
      { placeholder: true, label: "Hens settling in" },
      { placeholder: true, label: "Detail · door latch" },
      { placeholder: true, label: "Compost bin · finished" },
    ],
    reactionCounts: { like: 4, love: 2 },
    viewerKinds: [],
    commentCount: 3,
    topComments: [
      { author: "Eve Erickson", body: "Looks great! Did you do the framing yourself?" },
      { author: "Bob Bartholomew", body: "Sending this to my dad — he's been threatening to build one for years." },
    ],
  },
  {
    id: "2",
    author: "Grant DeFayette",
    handle: "grant",
    initials: "GD",
    time: "3/8/2026",
    visibility: "public",
    category: "Family",
    bodyHtml: "<p>Thank you everyone for the birthday wishes! I hope you all had a great weekend. We took the dog to the lake and she <em>finally</em> figured out swimming.</p>",
    reactionCounts: { love: 3 },
    viewerKinds: ["love"],
    commentCount: 1,
    topComments: [
      { author: "Carol Chen", body: "Happy birthday again, Grant! 🎂" },
    ],
  },
  {
    id: "3",
    author: "Eve Erickson",
    handle: "eve",
    initials: "EE",
    time: "5/21/2024",
    visibility: "private",
    bodyHtml: "<p>Trying out the private tier — apparently this means only I can see it. Useful for drafting before I actually share something.</p><p>Anyway: <a href=\"#\">this article</a> about preserving family photos is the kind of thing I'd post if I were a poster.</p>",
    reactionCounts: {},
    viewerKinds: [],
    commentCount: 0,
  },
  {
    id: "4",
    author: "Grant DeFayette",
    handle: "grant",
    initials: "GD",
    time: "3/21/2024",
    visibility: "posters",
    bodyHtml: "<p>My last Facebook post was in 2021. It's still 100% valid. Better now really.</p>",
    reactionCounts: { like: 2, haha: 1 },
    viewerKinds: ["like"],
    commentCount: 0,
  },
  {
    id: "5",
    author: "Grant DeFayette",
    handle: "grant",
    initials: "GD",
    time: "8/30/2021",
    visibility: "posters",
    category: "Garden",
    bodyHtml: "<p>Here's your daily dose of \"old man plays harmonica in a park while two ducks watch and sing along.\"</p>",
    media: [{ placeholder: true, label: "Video · 0:42 · ▶" }],
    reactionCounts: { haha: 5, love: 1 },
    viewerKinds: [],
    commentCount: 2,
    topComments: [
      { author: "Bob Bartholomew", body: "This is the content I subscribed for." },
    ],
  },
];

function App() {
  const [route, setRoute] = useState("feed");
  const [user, setUser] = useState({
    displayName: "Grant DeFayette",
    handle: "grant",
    initials: "GD",
    role: "administrator",
    email: "grant@cavebatsofware.com",
  });
  const [activeCategory, setActiveCategory] = useState(null);
  const [openPost, setOpenPost] = useState(null);
  const [lightbox, setLightbox] = useState(null); // { post, index }
  const [showInvite, setShowInvite] = useState(false);

  const visible = useMemo(() => {
    if (!activeCategory) return SAMPLE_POSTS;
    const name = { family: "Family", garden: "Garden", "trip-2025": "Trip 2025", "kids-art": "Kids' art" }[activeCategory];
    return SAMPLE_POSTS.filter((p) => p.category === name);
  }, [activeCategory]);

  function handleSignOut() { setUser(null); setRoute("login"); }
  function handleLogin(u)  { setUser({ displayName: u.email.split("@")[0], handle: u.email.split("@")[0], initials: u.email.slice(0, 2).toUpperCase(), role: "administrator", email: u.email }); setRoute("feed"); }

  return (
    <div className="layout">
      <Header route={route} onRoute={setRoute} user={user} onSignOut={handleSignOut} />

      <div className="layout-grid">
        <aside className="layout-rail">
          <BrowseRail activeCategory={activeCategory} onCategory={(c) => { setActiveCategory(c); setRoute("feed"); setOpenPost(null); }} />
        </aside>

        <main className="layout-main">
          {route === "feed" && !openPost && (
            <>
              <h1 className="sr-only">Feed</h1>
              <form className="feed-search" role="search" onSubmit={(e) => e.preventDefault()}>
                <input className="feed-search-input" type="search" placeholder="Search posts…" aria-label="Search" />
              </form>
              <section className="feed-list">
                {visible.map((p) => (
                  <PostCard
                    key={p.id}
                    post={p}
                    variant="feed"
                    onOpen={(post) => setOpenPost(post)}
                    onOpenMedia={(post, i) => setLightbox({ post, index: i })}
                  />
                ))}
              </section>
            </>
          )}

          {route === "feed" && openPost && (
            <div>
              <button className="post-meta-open" onClick={() => setOpenPost(null)} style={{ marginBottom: "var(--spacing-4)", display: "inline-block" }}>← Back to feed</button>
              <PostCard
                post={openPost}
                variant="permalink"
                onOpenMedia={(post, i) => setLightbox({ post, index: i })}
              />
              <div style={{ marginTop: "var(--spacing-8)" }}>
                <h2 style={{ fontSize: "var(--font-size-lg)", margin: "0 0 var(--spacing-4)", color: "var(--color-text)" }}>
                  Comments {openPost.commentCount ? `(${openPost.commentCount})` : ""}
                </h2>
                {openPost.topComments && openPost.topComments.length > 0 ? (
                  <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: "var(--spacing-4)" }}>
                    {openPost.topComments.map((c, i) => (
                      <li key={i} style={{ background: "var(--color-surface)", border: "1px solid var(--color-border)", borderRadius: "var(--radius-md)", padding: "var(--spacing-4)" }}>
                        <div style={{ fontWeight: 600, marginBottom: "var(--spacing-1)" }}>{c.author}</div>
                        <div>{c.body}</div>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p style={{ color: "var(--color-muted)" }}>No comments yet.</p>
                )}
              </div>
            </div>
          )}

          {route === "compose" && (
            <Composer
              onPublish={() => setRoute("feed")}
              onCancel={() => setRoute("feed")}
            />
          )}

          {route === "login" && (
            <Login onLogin={handleLogin} onCancel={() => setRoute("feed")} />
          )}

          {route === "new-album" && (
            <div className="compose-card">
              <h1 style={{ margin: 0, fontSize: "var(--font-size-2xl)", color: "var(--color-primary)" }}>New album</h1>
              <p style={{ color: "var(--color-muted)" }}>Album composition is out of scope for this UI kit. See <code>_source/pages/ComposeAlbum.jsx</code> for the production version.</p>
              <button className="btn-secondary" style={{ alignSelf: "flex-start" }} onClick={() => setRoute("feed")}>← Back to feed</button>
            </div>
          )}
        </main>
      </div>

      {lightbox && (
        <Lightbox
          items={lightbox.post.media}
          index={lightbox.index}
          onIndex={(i) => setLightbox({ ...lightbox, index: i })}
          onClose={() => setLightbox(null)}
        />
      )}

      {showInvite && (
        <InviteSplash
          inviteEmail="friend@example.com"
          onAccept={() => { setShowInvite(false); setRoute("login"); }}
          onDecline={() => setShowInvite(false)}
        />
      )}

      {/* Demo helper: trigger invite splash */}
      {!showInvite && (
        <button
          onClick={() => setShowInvite(true)}
          style={{
            position: "fixed", bottom: 16, right: 16, zIndex: 5,
            background: "var(--color-surface)", color: "var(--color-muted)",
            border: "1px solid var(--color-border)", borderRadius: "var(--radius-md)",
            padding: "6px 12px", fontSize: "12px", cursor: "pointer", font: "inherit",
            boxShadow: "var(--shadow-popover)",
          }}
        >
          Demo: show invite splash
        </button>
      )}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<App />);

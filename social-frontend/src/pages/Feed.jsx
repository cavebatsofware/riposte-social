import { useAuth } from "../contexts/AuthContext";

export default function Feed() {
  const { user, loading } = useAuth();

  return (
    <main className="feed">
      <header className="feed-header">
        <h1>Riposte Social</h1>
        <p className="feed-subtitle">
          {loading
            ? "…"
            : user
              ? `Signed in as ${user.display_name || user.email}`
              : "A self-hosted social site for family and close friends."}
        </p>
      </header>
      <section className="feed-empty">
        <p>Feed coming soon.</p>
      </section>
    </main>
  );
}

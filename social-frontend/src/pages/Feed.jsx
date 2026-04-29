import { Link } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import InviteSplash from "../components/InviteSplash";

export default function Feed() {
  const { user, loading, logout } = useAuth();

  return (
    <main className="feed">
      <header className="feed-header">
        <div className="feed-header-row">
          <h1>Riposte Social</h1>
          {!loading &&
            (user ? (
              <button
                type="button"
                className="btn-secondary"
                onClick={logout}
              >
                Sign out
              </button>
            ) : (
              <Link to="/login" className="btn-primary">
                Sign in
              </Link>
            ))}
        </div>
        <p className="feed-subtitle">
          {loading
            ? "…"
            : user
              ? `Signed in as ${user.display_name || user.email}`
              : "A self-hosted social site for family and close friends."}
        </p>
      </header>

      <InviteSplash />

      <section className="feed-empty">
        <p>Feed coming soon.</p>
      </section>
    </main>
  );
}

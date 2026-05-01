import { useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import Layout from "../components/Layout";
import "./Settings.css";

const BIO_MAX = 500;
const PRONOUNS_MAX = 30;

/// `/settings/profile` — self-service editor for the caller's profile.
///
/// Loads `GET /api/me/profile` on mount, lets the user edit handle /
/// display name / bio / pronouns, and posts changes via `PATCH
/// /api/me/profile`. Avatar uploads use `POST /api/me/avatar` (multipart);
/// removal uses `DELETE /api/me/avatar`.
///
/// Visiting this page without an authenticated session redirects to
/// `/login` — server-side endpoints would 401 otherwise, but redirecting
/// up front avoids a flash of the form and the subsequent error.
export default function SettingsProfile() {
  const { user, loading: authLoading, refreshUser } = useAuth();
  const navigate = useNavigate();
  const fileInputRef = useRef(null);

  const [profile, setProfile] = useState(null);
  const [loadingProfile, setLoadingProfile] = useState(true);

  const [handle, setHandle] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [bio, setBio] = useState("");
  const [pronouns, setPronouns] = useState("");

  const [savingProfile, setSavingProfile] = useState(false);
  const [uploadingAvatar, setUploadingAvatar] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");

  useEffect(() => {
    if (!authLoading && !user) {
      navigate("/login", { replace: true });
    }
  }, [authLoading, user, navigate]);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoadingProfile(true);
      setError("");
      try {
        const response = await fetchApi("/api/me/profile");
        if (!response.ok) throw new Error("Failed to load profile");
        const data = await response.json();
        if (cancelled) return;
        setProfile(data);
        setHandle(data.handle || "");
        setDisplayName(data.display_name || "");
        setBio(data.bio || "");
        setPronouns(data.pronouns || "");
      } catch (err) {
        if (!cancelled) setError(err.message);
      } finally {
        if (!cancelled) setLoadingProfile(false);
      }
    }
    if (user) load();
    return () => {
      cancelled = true;
    };
  }, [user]);

  async function handleSubmit(e) {
    e.preventDefault();
    setSavingProfile(true);
    setError("");
    setSuccess("");
    try {
      const body = {};
      if (handle !== profile.handle) body.handle = handle;
      if (displayName !== (profile.display_name || ""))
        body.display_name = displayName;
      if (bio !== (profile.bio || "")) body.bio = bio;
      if (pronouns !== (profile.pronouns || "")) body.pronouns = pronouns;

      if (Object.keys(body).length === 0) {
        setSuccess("No changes to save.");
        setSavingProfile(false);
        return;
      }

      const response = await fetchApi("/api/me/profile", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to save profile");
      }
      const updated = await response.json();
      setProfile(updated);
      setHandle(updated.handle || "");
      setDisplayName(updated.display_name || "");
      setBio(updated.bio || "");
      setPronouns(updated.pronouns || "");
      setSuccess("Profile saved.");
      if (refreshUser) await refreshUser();
    } catch (err) {
      setError(err.message);
    } finally {
      setSavingProfile(false);
    }
  }

  async function handleAvatarPick(e) {
    const file = e.target.files?.[0];
    if (!file) return;
    setUploadingAvatar(true);
    setError("");
    setSuccess("");
    try {
      const form = new FormData();
      form.append("file", file);
      const response = await fetchApi("/api/me/avatar", {
        method: "POST",
        body: form,
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to upload avatar");
      }
      const data = await response.json();
      setProfile((prev) => prev && { ...prev, avatar_url: data.avatar_url });
      setSuccess("Avatar updated.");
      if (refreshUser) await refreshUser();
    } catch (err) {
      setError(err.message);
    } finally {
      setUploadingAvatar(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  }

  async function handleAvatarRemove() {
    if (!window.confirm("Remove your avatar?")) return;
    setUploadingAvatar(true);
    setError("");
    setSuccess("");
    try {
      const response = await fetchApi("/api/me/avatar", { method: "DELETE" });
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to remove avatar");
      }
      setProfile((prev) => prev && { ...prev, avatar_url: null });
      setSuccess("Avatar removed.");
      if (refreshUser) await refreshUser();
    } catch (err) {
      setError(err.message);
    } finally {
      setUploadingAvatar(false);
    }
  }

  if (authLoading || loadingProfile || !profile) {
    return (
      <Layout>
        <p className="muted">Loading…</p>
      </Layout>
    );
  }

  return (
    <Layout>
      <header className="settings-header">
        <h1>Profile settings</h1>
        <nav className="settings-tabs" aria-label="Settings sections">
          <Link to="/settings/profile" className="settings-tab active">
            Profile
          </Link>
          <Link to="/settings/security" className="settings-tab">
            Security
          </Link>
        </nav>
      </header>

      {error && <div className="alert alert-error">{error}</div>}
      {success && <div className="alert alert-success">{success}</div>}

      <section className="settings-section">
        <h2>Avatar</h2>
        <div className="settings-avatar-row">
          <div className="settings-avatar-preview">
            {profile.avatar_url ? (
              <img src={profile.avatar_url} alt="Your avatar" />
            ) : (
              <span aria-hidden="true">
                {(profile.display_name || profile.handle || "??")
                  .slice(0, 2)
                  .toUpperCase()}
              </span>
            )}
          </div>
          <div className="settings-avatar-actions">
            <input
              ref={fileInputRef}
              type="file"
              accept="image/jpeg,image/png,image/webp"
              onChange={handleAvatarPick}
              disabled={uploadingAvatar}
            />
            {profile.avatar_url && (
              <button
                type="button"
                className="btn-secondary"
                onClick={handleAvatarRemove}
                disabled={uploadingAvatar}
              >
                Remove avatar
              </button>
            )}
            <p className="form-hint">
              JPEG, PNG, or WebP up to 5 MB. We'll center-crop to a square.
            </p>
          </div>
        </div>
      </section>

      <form className="settings-form" onSubmit={handleSubmit}>
        <h2>Profile fields</h2>
        <label htmlFor="settings-handle">Handle</label>
        <input
          id="settings-handle"
          type="text"
          value={handle}
          onChange={(e) => setHandle(e.target.value)}
          minLength={3}
          maxLength={30}
          pattern="[a-z0-9_\-]+"
          autoComplete="off"
          required
        />
        <p className="form-hint">
          3–30 characters. Lowercase letters, digits, underscore, hyphen.
        </p>

        <label htmlFor="settings-display-name">Display name</label>
        <input
          id="settings-display-name"
          type="text"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          maxLength={100}
        />

        <label htmlFor="settings-pronouns">Pronouns</label>
        <input
          id="settings-pronouns"
          type="text"
          value={pronouns}
          onChange={(e) => setPronouns(e.target.value)}
          maxLength={PRONOUNS_MAX}
        />

        <label htmlFor="settings-bio">Bio</label>
        <textarea
          id="settings-bio"
          value={bio}
          onChange={(e) => setBio(e.target.value)}
          rows={4}
          maxLength={BIO_MAX}
        />
        <p className="form-hint">
          {bio.length}/{BIO_MAX} characters
        </p>

        <div className="settings-form-actions">
          <button
            type="submit"
            className="btn-primary"
            disabled={savingProfile}
          >
            {savingProfile ? "Saving…" : "Save changes"}
          </button>
          {profile.handle && (
            <Link to={`/u/${profile.handle}`} className="btn-secondary">
              View profile
            </Link>
          )}
        </div>
      </form>
    </Layout>
  );
}

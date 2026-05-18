import { useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../../contexts/AuthContext";
import {
  deleteAvatar,
  fetchMyProfile,
  updateMyProfile,
  uploadAvatar,
} from "./api";
import Layout from "../../components/Layout";
import "./Settings.css";

const BIO_MAX = 500;
const PRONOUNS_MAX = 30;

/// `/settings/profile`: self-service editor for the caller's profile.
///
/// Loads `GET /api/me/profile` on mount, lets the user edit handle,
/// display name, bio, and pronouns, and posts changes via `PATCH
/// /api/me/profile`. Avatar uploads use `POST /api/me/avatar` (multipart);
/// removal uses `DELETE /api/me/avatar`.
///
/// Visiting this page without an authenticated session redirects to
/// `/login`. Server-side endpoints would 401 otherwise; redirecting up
/// front avoids a flash of the form and the subsequent error.
export default function SettingsProfile() {
  const { user, loading: authLoading, refreshUser } = useAuth();
  const navigate = useNavigate();
  const fileInputRef = useRef(null);
  const { t } = useTranslation("settings");
  const { t: tCommon } = useTranslation("common");

  const [profile, setProfile] = useState<{ handle: string; display_name?: string; bio?: string; pronouns?: string; avatar_url?: string } | null>(null);
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
        const response = await fetchMyProfile();
        if (!response.ok) throw new Error(t("profile.loadFailed"));
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
      const body: { handle?: string; display_name?: string; bio?: string; pronouns?: string } = {};
      if (handle !== profile.handle) body.handle = handle;
      if (displayName !== (profile.display_name || ""))
        body.display_name = displayName;
      if (bio !== (profile.bio || "")) body.bio = bio;
      if (pronouns !== (profile.pronouns || "")) body.pronouns = pronouns;

      if (Object.keys(body).length === 0) {
        setSuccess(t("profile.noChanges"));
        setSavingProfile(false);
        return;
      }

      const response = await updateMyProfile(body);
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("profile.saveFailed"));
      }
      const updated = await response.json();
      setProfile(updated);
      setHandle(updated.handle || "");
      setDisplayName(updated.display_name || "");
      setBio(updated.bio || "");
      setPronouns(updated.pronouns || "");
      setSuccess(t("profile.savedSuccess"));
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
      const response = await uploadAvatar(form);
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("profile.uploadFailed"));
      }
      const data = await response.json();
      setProfile((prev) => prev && { ...prev, avatar_url: data.avatar_url });
      setSuccess(t("profile.uploadedSuccess"));
      if (refreshUser) await refreshUser();
    } catch (err) {
      setError(err.message);
    } finally {
      setUploadingAvatar(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  }

  async function handleAvatarRemove() {
    if (!window.confirm(t("profile.removeConfirm"))) return;
    setUploadingAvatar(true);
    setError("");
    setSuccess("");
    try {
      const response = await deleteAvatar();
      if (!response.ok && response.status !== 204) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || t("profile.removeFailed"));
      }
      setProfile((prev) => prev && { ...prev, avatar_url: null });
      setSuccess(t("profile.removedSuccess"));
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
        <p className="muted">{tCommon("loading")}</p>
      </Layout>
    );
  }

  return (
    <Layout>
      <header className="settings-header">
        <h1>{t("profile.title")}</h1>
        <nav className="settings-tabs" aria-label={t("tabsAria")}>
          <Link
            to="/settings/profile"
            className="settings-tab active"
            aria-current="page"
          >
            {t("tabProfile")}
          </Link>
          <Link to="/settings/security" className="settings-tab">
            {t("tabSecurity")}
          </Link>
        </nav>
      </header>

      {error && (
        <div className="alert alert-error" role="alert">
          {error}
        </div>
      )}
      {success && (
        <div className="alert alert-success" role="status">
          {success}
        </div>
      )}

      <section className="settings-section">
        <h2>{t("profile.avatarHeading")}</h2>
        <div className="settings-avatar-row">
          <div className="settings-avatar-preview">
            {profile.avatar_url ? (
              <img src={profile.avatar_url} alt={t("profile.avatarAlt")} />
            ) : (
              <span aria-hidden="true">
                {(profile.display_name || profile.handle || "??")
                  .slice(0, 2)
                  .toUpperCase()}
              </span>
            )}
          </div>
          <div className="settings-avatar-actions">
            <label htmlFor="settings-avatar-input" className="sr-only">
              {t("profile.avatarUploadLabel")}
            </label>
            <input
              ref={fileInputRef}
              id="settings-avatar-input"
              name="avatar"
              type="file"
              accept="image/jpeg,image/png,image/webp"
              onChange={handleAvatarPick}
              disabled={uploadingAvatar}
              aria-describedby="settings-avatar-hint"
            />
            {profile.avatar_url && (
              <button
                type="button"
                className="btn-secondary"
                onClick={handleAvatarRemove}
                disabled={uploadingAvatar}
              >
                {t("profile.removeAvatar")}
              </button>
            )}
            <p id="settings-avatar-hint" className="form-hint">
              {t("profile.avatarHint")}
            </p>
          </div>
        </div>
      </section>

      <form
        className="settings-form"
        onSubmit={handleSubmit}
        aria-busy={savingProfile}
        aria-labelledby="settings-fields-heading"
      >
        <h2 id="settings-fields-heading">{t("profile.fieldsHeading")}</h2>
        <label htmlFor="settings-handle">{t("profile.handleLabel")}</label>
        <input
          id="settings-handle"
          name="handle"
          type="text"
          value={handle}
          onChange={(e) => setHandle(e.target.value)}
          minLength={3}
          maxLength={30}
          pattern="[a-z0-9_\-]+"
          autoComplete="off"
          required
        />
        <p className="form-hint">{t("profile.handleHint")}</p>

        <label htmlFor="settings-display-name">
          {t("profile.displayNameLabel")}
        </label>
        <input
          id="settings-display-name"
          name="display_name"
          type="text"
          autoComplete="nickname"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          maxLength={100}
        />

        <label htmlFor="settings-pronouns">{t("profile.pronounsLabel")}</label>
        <input
          id="settings-pronouns"
          name="pronouns"
          type="text"
          value={pronouns}
          onChange={(e) => setPronouns(e.target.value)}
          maxLength={PRONOUNS_MAX}
        />

        <label htmlFor="settings-bio">{t("profile.bioLabel")}</label>
        <textarea
          id="settings-bio"
          name="bio"
          value={bio}
          onChange={(e) => setBio(e.target.value)}
          rows={4}
          maxLength={BIO_MAX}
          aria-describedby="settings-bio-count"
        />
        <p
          id="settings-bio-count"
          className="form-hint"
          aria-live="polite"
          aria-atomic="true"
        >
          {t("profile.bioCount", { current: bio.length, max: BIO_MAX })}
        </p>

        <div className="settings-form-actions">
          <button
            type="submit"
            className="btn-primary"
            disabled={savingProfile}
          >
            {savingProfile
              ? tCommon("actions.saving")
              : t("profile.saveChanges")}
          </button>
          {profile.handle && (
            <Link to={`/u/${profile.handle}`} className="btn-secondary">
              {t("profile.viewProfile")}
            </Link>
          )}
        </div>
      </form>
    </Layout>
  );
}

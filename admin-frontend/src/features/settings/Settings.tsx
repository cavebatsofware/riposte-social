import React, { useState, useEffect } from "react";
import Layout from "../../components/Layout";
import { useAuth } from "../../contexts/AuthContext";
import { fetchSettings as fetchSettingsApi, updateSetting } from "./api";
import "./Settings.css";

function Settings() {
  const { refreshUser } = useAuth();
  const [settings, setSettings] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const [editingValues, setEditingValues] = useState({});
  const [savingStates, setSavingStates] = useState({}); // Track saving/saved per setting
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [newSetting, setNewSetting] = useState({
    key: "",
    value: "false",
    category: "system",
  });

  useEffect(() => {
    fetchSettings();
  }, []);

  async function fetchSettings() {
    try {
      setLoading(true);
      const response = await fetchSettingsApi();

      if (!response.ok) {
        throw new Error("Failed to fetch settings");
      }

      const data = await response.json();
      setSettings(data);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function handleToggleSetting(setting) {
    setSaving(true);
    setError("");

    try {
      const newValue = setting.value === "true" ? "false" : "true";

      const response = await updateSetting({
        key: setting.key,
        value: newValue,
        category: setting.category,
      });

      if (!response.ok) {
        throw new Error("Failed to update setting");
      }

      setSettings(
        settings.map((s) =>
          s.id === setting.id ? { ...s, value: newValue } : s,
        ),
      );

      // Refresh user data so feature flags update in nav/UI immediately
      refreshUser();
    } catch (err) {
      setError(err.message);
    } finally {
      setSaving(false);
    }
  }

  function handleTextChange(settingId, newValue) {
    setEditingValues({
      ...editingValues,
      [settingId]: newValue,
    });
  }

  async function saveTextSetting(setting) {
    const newValue = editingValues[setting.id];
    if (newValue === undefined || newValue === setting.value) {
      return; // No change
    }

    setSaving(true);
    setSavingStates({ ...savingStates, [setting.id]: "saving" });
    setError("");

    try {
      const response = await updateSetting({
        key: setting.key,
        value: newValue,
        category: setting.category,
      });

      if (!response.ok) {
        throw new Error("Failed to update setting");
      }

      setSettings(
        settings.map((s) =>
          s.id === setting.id
            ? isSecretSetting(s)
              ? { ...s, value: "", has_value: true }
              : { ...s, value: newValue }
            : s,
        ),
      );

      // Clear editing state
      const newEditingValues = { ...editingValues };
      delete newEditingValues[setting.id];
      setEditingValues(newEditingValues);

      // Show saved indicator
      setSavingStates({ ...savingStates, [setting.id]: "saved" });

      // Clear saved indicator after 2 seconds
      setTimeout(() => {
        setSavingStates((prev) => {
          const updated = { ...prev };
          delete updated[setting.id];
          return updated;
        });
      }, 2000);
    } catch (err) {
      setError(err.message);
      setSavingStates({ ...savingStates, [setting.id]: "error" });
    } finally {
      setSaving(false);
    }
  }

  function getSettingLabel(key) {
    const labels = {
      admin_registration_enabled: "Admin Registration",
      access_codes_enabled: "Access Codes",
      contact_form_enabled: "Contact Form",
      subscriptions_enabled: "Newsletter Subscriptions",
      poster_posting_enabled: "Poster Posting",
      poster_category_management_enabled: "Poster Category Management",
      commenter_invites_enabled: "Commenter Invites",
      public_feed_enabled: "Public Feed",
      fb_import_enabled: "Facebook Import",
      site_name: "Site Name",
      site_domain: "Site Domain",
      contact_email: "Contact Email",
      from_email: "From Email",
      max_image_dimension: "Max Image Dimension",
    };
    return labels[key] || key;
  }

  function getSettingDescription(key) {
    const descriptions = {
      admin_registration_enabled:
        "Allow new administrators to register accounts via the registration page",
      access_codes_enabled:
        "Enable code-gated document access for the public site",
      contact_form_enabled: "Enable the public contact form endpoint",
      subscriptions_enabled:
        "Enable the public newsletter subscription endpoint",
      poster_posting_enabled:
        "Allow users with the Poster role to create new posts. Administrators always retain posting access.",
      poster_category_management_enabled:
        "Allow users with the Poster role to create, edit, delete, and manage membership of categories they own. Administrators always retain full category management.",
      commenter_invites_enabled:
        "Master switch for the invite system. When off, administrators cannot issue new invites AND existing un-accepted invites can no longer be redeemed (both OIDC and password modes). Already-activated commenter accounts continue to log in normally.",
      public_feed_enabled:
        "Allow anonymous visitors to read the public feed. When disabled, the site becomes invite-only and visitors see a sign-in prompt.",
      fb_import_enabled:
        "Allow administrators to upload new Facebook archives. Existing import jobs are unaffected.",
      site_name: "The name of your website displayed in emails and pages",
      site_domain: "The domain name of your website (e.g., example.com)",
      contact_email: "Email address for contact form submissions",
      from_email: "Email address used as the sender for outgoing emails",
      max_image_dimension:
        "Largest width or height in pixels accepted for an uploaded image (post media and avatars). Inputs over the limit are rejected before decode to bound server memory. 8000 covers typical phone/DSLR photos at ~256 MiB worst case; raise for a photography-oriented site, lower (e.g. 2048) for tighter storage.",
    };
    return descriptions[key] || "";
  }

  function isToggleSetting(key, value) {
    // Boolean settings are those with "true" or "false" values
    return value === "true" || value === "false";
  }

  function isSecretSetting(setting) {
    return setting.encrypted === true || setting.key.startsWith("secret_");
  }

  function hasUnsavedChanges(settingId) {
    return (
      editingValues[settingId] !== undefined &&
      editingValues[settingId] !== settings.find((s) => s.id === settingId)?.value
    );
  }

  function getSaveStatus(settingId) {
    if (savingStates[settingId] === "saving") {
      return <span className="save-status saving">Saving...</span>;
    }
    if (savingStates[settingId] === "saved") {
      return <span className="save-status saved">✓ Saved</span>;
    }
    if (hasUnsavedChanges(settingId)) {
      return (
        <button
          className="btn-save"
          onClick={() => {
            const setting = settings.find((s) => s.id === settingId);
            if (setting) saveTextSetting(setting);
          }}
          disabled={saving}
        >
          Save
        </button>
      );
    }
    return null;
  }

  if (loading) {
    return <div className="loading">Loading settings...</div>;
  }

  return (
    <Layout>
      <div className="settings-page">
        <header className="page-header">
          <h1>System Settings</h1>
        </header>

        {error && <div className="error">{error}</div>}

        <div className="settings-list">
          {settings.length === 0 ? (
            <div className="empty-state">
              <p>No settings configured.</p>
            </div>
          ) : (
            settings.map((setting) => (
              <div key={setting.id} className="setting-item">
                <div className="setting-info">
                  <div className="setting-label">
                    {getSettingLabel(setting.key)}
                  </div>
                  {getSettingDescription(setting.key) && (
                    <div className="setting-description">
                      {getSettingDescription(setting.key)}
                    </div>
                  )}
                  {setting.category && (
                    <div className="setting-category">
                      <span className="badge">{setting.category}</span>
                    </div>
                  )}
                </div>
                <div className="setting-control">
                  {isToggleSetting(setting.key, setting.value) ? (
                    <>
                      <label className="toggle-switch">
                        <input
                          type="checkbox"
                          checked={setting.value === "true"}
                          onChange={() => handleToggleSetting(setting)}
                          disabled={saving}
                        />
                        <span className="toggle-slider"></span>
                      </label>
                      <span className="setting-value">
                        {setting.value === "true" ? "Enabled" : "Disabled"}
                      </span>
                    </>
                  ) : isSecretSetting(setting) ? (
                    <div className="text-input-container">
                      <input
                        type="password"
                        className="text-input"
                        autoComplete="new-password"
                        value={
                          editingValues[setting.id] !== undefined
                            ? editingValues[setting.id]
                            : ""
                        }
                        onChange={(e) =>
                          handleTextChange(setting.id, e.target.value)
                        }
                        onBlur={() => saveTextSetting(setting)}
                        disabled={saving}
                        placeholder={setting.has_value ? "(set)" : "Not set"}
                      />
                      {getSaveStatus(setting.id)}
                    </div>
                  ) : (
                    <div className="text-input-container">
                      <input
                        type="text"
                        className="text-input"
                        value={
                          editingValues[setting.id] !== undefined
                            ? editingValues[setting.id]
                            : setting.value
                        }
                        onChange={(e) =>
                          handleTextChange(setting.id, e.target.value)
                        }
                        onBlur={() => saveTextSetting(setting)}
                        disabled={saving}
                        placeholder={getSettingLabel(setting.key)}
                      />
                      {getSaveStatus(setting.id)}
                    </div>
                  )}
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </Layout>
  );
}

export default Settings;

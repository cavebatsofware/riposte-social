import React, { useState, useEffect } from "react";
import Layout from "../components/Layout";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
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
      const response = await fetchApi("/api/admin/settings");

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

      const response = await fetchApi("/api/admin/settings", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          key: setting.key,
          value: newValue,
          category: setting.category,
        }),
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
      const response = await fetchApi("/api/admin/settings", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          key: setting.key,
          value: newValue,
          category: setting.category,
        }),
      });

      if (!response.ok) {
        throw new Error("Failed to update setting");
      }

      setSettings(
        settings.map((s) =>
          s.id === setting.id ? { ...s, value: newValue } : s,
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
      site_name: "Site Name",
      site_domain: "Site Domain",
      contact_email: "Contact Email",
      from_email: "From Email",
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
      site_name: "The name of your website displayed in emails and pages",
      site_domain: "The domain name of your website (e.g., example.com)",
      contact_email: "Email address for contact form submissions",
      from_email: "Email address used as the sender for outgoing emails",
    };
    return descriptions[key] || "";
  }

  function isToggleSetting(key, value) {
    // Boolean settings are those with "true" or "false" values
    return value === "true" || value === "false";
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

import React, { useState, useEffect } from "react";
import { useSearchParams, useNavigate } from "react-router-dom";
import Layout from "../components/Layout";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import "./AccessCodes.css";

function AccessCodes() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const { user } = useAuth();

  // Redirect if access codes feature is disabled
  useEffect(() => {
    if (user?.features?.access_codes_enabled === false) {
      navigate("/dashboard", { replace: true });
    }
  }, [user, navigate]);
  const [codes, setCodes] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [showDescriptionModal, setShowDescriptionModal] = useState(false);
  const [selectedDescription, setSelectedDescription] = useState(null);
  const [newCode, setNewCode] = useState({
    code: "",
    name: "",
    description: "",
    download_filename: "",
    expires_at: "",
    index_html: null,
    document_docx: null,
  });

  useEffect(() => {
    fetchCodes();
  }, []);

  useEffect(() => {
    // Scroll to highlighted code if present in URL
    const highlightId = searchParams.get("highlight");
    if (highlightId && codes.length > 0) {
      const element = document.getElementById(`code-${highlightId}`);
      if (element) {
        element.scrollIntoView({ behavior: "smooth", block: "center" });
        element.classList.add("highlighted");
        setTimeout(() => element.classList.remove("highlighted"), 3000);
      }
    }
  }, [searchParams, codes]);

  function generateRandomCode() {
    // Generate a secure random code using crypto API
    // Format: 12 characters of base62 (alphanumeric, case-sensitive)
    const array = new Uint8Array(12);
    crypto.getRandomValues(array);

    // Convert to base62 (0-9, a-z, A-Z)
    const base62 =
      "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let code = "";
    for (let i = 0; i < array.length; i++) {
      code += base62[array[i] % base62.length];
    }

    return code;
  }

  function handleGenerateCode() {
    setNewCode({ ...newCode, code: generateRandomCode() });
  }

  async function fetchCodes() {
    try {
      const response = await fetchApi("/api/admin/access-codes");

      if (!response.ok) {
        throw new Error("Failed to fetch access codes");
      }

      const data = await response.json();
      setCodes(data);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function handleCreateCode(e) {
    e.preventDefault();
    setError("");

    try {
      // Validate files are selected
      if (!newCode.index_html || !newCode.document_docx) {
        throw new Error("Both index.html and Document.docx files are required");
      }

      // Create FormData for multipart upload
      const formData = new FormData();
      formData.append("code", newCode.code);
      formData.append("name", newCode.name);
      if (newCode.description) {
        formData.append("description", newCode.description);
      }
      if (newCode.download_filename) {
        formData.append("download_filename", newCode.download_filename);
      }
      if (newCode.expires_at) {
        formData.append("expires_at", newCode.expires_at);
      }
      formData.append("index_html", newCode.index_html);
      formData.append("document_docx", newCode.document_docx);

      const response = await fetchApi("/api/admin/access-codes", {
        method: "POST",
        body: formData,
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to create access code");
      }

      // Reset form and refresh list
      setNewCode({
        code: "",
        name: "",
        description: "",
        download_filename: "",
        expires_at: "",
        index_html: null,
        document_docx: null,
      });
      setShowCreateForm(false);
      await fetchCodes();
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleDeleteCode(id) {
    if (!confirm("Are you sure you want to delete this access code?")) {
      return;
    }

    try {
      const response = await fetchApi(`/api/admin/access-codes/${id}`, {
        method: "DELETE",
      });

      if (!response.ok) {
        throw new Error("Failed to delete access code");
      }

      await fetchCodes();
    } catch (err) {
      setError(err.message);
    }
  }

  function formatDate(dateString) {
    if (!dateString) return "Never";
    const date = new Date(dateString);
    return date.toLocaleDateString() + " " + date.toLocaleTimeString();
  }

  if (loading) {
    return <div className="loading">Loading access codes...</div>;
  }

  return (
    <Layout>
      <div className="access-codes-page">
        <header className="page-header">
          <h1>Access Code Management</h1>
          <button
            onClick={() => setShowCreateForm(!showCreateForm)}
            className="btn-primary"
          >
            {showCreateForm ? "Cancel" : "+ New Access Code"}
          </button>
        </header>

        {error && <div className="error">{error}</div>}

        {showCreateForm && (
          <div className="create-form-container">
            <form onSubmit={handleCreateCode} className="create-form">
              <h2>Create New Access Code</h2>

              <div className="form-group">
                <label htmlFor="code">Access Code *</label>
                <div className="input-with-button">
                  <input
                    type="text"
                    id="code"
                    value={newCode.code}
                    onChange={(e) =>
                      setNewCode({ ...newCode, code: e.target.value })
                    }
                    required
                    placeholder="e.g., document-2025"
                  />
                  <button
                    type="button"
                    onClick={handleGenerateCode}
                    className="btn-generate"
                  >
                    Generate
                  </button>
                </div>
              </div>

              <div className="form-group">
                <label htmlFor="name">Name/Description *</label>
                <input
                  type="text"
                  id="name"
                  value={newCode.name}
                  onChange={(e) =>
                    setNewCode({ ...newCode, name: e.target.value })
                  }
                  required
                  placeholder="e.g., Personal Link"
                />
              </div>

              <div className="form-group">
                <label htmlFor="description">
                  Extended Description (Optional)
                </label>
                <textarea
                  id="description"
                  value={newCode.description}
                  onChange={(e) =>
                    setNewCode({ ...newCode, description: e.target.value })
                  }
                  rows="8"
                  placeholder="Add a longer description here (a few pages of text)"
                />
                <small>Optional longer description for this access code</small>
              </div>

              <div className="form-group">
                <label htmlFor="download_filename">
                  Download Filename (Optional)
                </label>
                <input
                  type="text"
                  id="download_filename"
                  value={newCode.download_filename}
                  onChange={(e) =>
                    setNewCode({
                      ...newCode,
                      download_filename: e.target.value,
                    })
                  }
                  placeholder="e.g., John_Doe_Document"
                />
                <small>
                  Custom filename for downloads. Leave empty for default.
                </small>
              </div>

              <div className="form-group">
                <label htmlFor="expires_at">Expiration Date (Optional)</label>
                <input
                  type="date"
                  id="expires_at"
                  value={newCode.expires_at}
                  onChange={(e) =>
                    setNewCode({ ...newCode, expires_at: e.target.value })
                  }
                />
                <small>
                  Leave empty for no expiration. Code expires at end of selected
                  day.
                </small>
              </div>

              <div className="form-group">
                <label htmlFor="index_html">Index HTML File *</label>
                <input
                  type="file"
                  id="index_html"
                  accept=".html"
                  onChange={(e) =>
                    setNewCode({ ...newCode, index_html: e.target.files[0] })
                  }
                  required
                />
                <small>The HTML page to display for this access code.</small>
              </div>

              <div className="form-group">
                <label htmlFor="document_docx">Document DOCX File *</label>
                <input
                  type="file"
                  id="document_docx"
                  accept=".docx"
                  onChange={(e) =>
                    setNewCode({ ...newCode, document_docx: e.target.files[0] })
                  }
                  required
                />
                <small>The DOCX template file for the document.</small>
              </div>

              <div className="form-actions">
                <button type="submit" className="btn-primary">
                  Create Code
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setShowCreateForm(false);
                    setNewCode({
                      code: "",
                      name: "",
                      description: "",
                      download_filename: "",
                      expires_at: "",
                      index_html: null,
                      document_docx: null,
                    });
                  }}
                  className="btn-secondary"
                >
                  Cancel
                </button>
              </div>
            </form>
          </div>
        )}

        <div className="codes-list">
          <h2>Active Access Codes ({codes.length})</h2>

          {codes.length === 0 ? (
            <div className="empty-state">
              <p>No access codes yet. Create one to get started!</p>
            </div>
          ) : (
            <div className="codes-grid">
              {codes.map((code) => (
                <div
                  key={code.id}
                  id={`code-${code.id}`}
                  className={`code-card ${code.is_expired ? "expired" : ""}`}
                >
                  <div className="code-header">
                    <h3>{code.name}</h3>
                    {code.is_expired && (
                      <span className="badge-expired">Expired</span>
                    )}
                  </div>

                  <div className="code-details">
                    <div className="code-value">
                      <strong>Code:</strong>
                      <code>{code.code}</code>
                    </div>

                    <div className="code-meta">
                      {code.download_filename && (
                        <div>
                          <strong>Download Filename: </strong>
                          {code.download_filename}.docx
                        </div>
                      )}
                      <div>
                        <strong>Expires:</strong> {formatDate(code.expires_at)}
                      </div>
                      <div>
                        <strong>Created:</strong> {formatDate(code.created_at)}
                      </div>
                      <div>
                        <strong>Usage Count:</strong> {code.usage_count || 0}
                      </div>
                    </div>
                  </div>

                  <div className="code-actions">
                    {code.description && (
                      <button
                        onClick={() => {
                          setSelectedDescription({
                            name: code.name,
                            description: code.description,
                          });
                          setShowDescriptionModal(true);
                        }}
                        className="btn-secondary"
                      >
                        View Description
                      </button>
                    )}
                    <button
                      onClick={() => handleDeleteCode(code.id)}
                      className="btn-delete"
                    >
                      Delete
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Description Modal */}
        {showDescriptionModal && selectedDescription && (
          <div
            className="modal-overlay"
            onClick={() => setShowDescriptionModal(false)}
          >
            <div className="modal-content" onClick={(e) => e.stopPropagation()}>
              <div className="modal-header">
                <h2>{selectedDescription.name}</h2>
                <button
                  className="modal-close"
                  onClick={() => setShowDescriptionModal(false)}
                >
                  ✕
                </button>
              </div>
              <div className="modal-body">
                <pre className="description-text">
                  {selectedDescription.description}
                </pre>
              </div>
              <div className="modal-footer">
                <button
                  className="btn-secondary"
                  onClick={() => setShowDescriptionModal(false)}
                >
                  Close
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </Layout>
  );
}

export default AccessCodes;

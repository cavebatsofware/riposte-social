import { useState, useEffect } from "react";
import Layout from "../../components/Layout";
import Table from "../../components/Table";
import {
  createInviteCode,
  fetchInviteCodes,
  fetchSiteConfig,
  revokeInviteCode,
} from "./api";
import "./InviteCodes.css";

const STATUS_BADGE = {
  active: "badge badge-success",
  used: "badge badge-info",
  expired: "badge badge-gray",
  revoked: "badge badge-danger",
};

function formatDate(value) {
  if (!value) return "";
  try {
    return new Date(value).toLocaleString();
  } catch {
    return value;
  }
}

function InviteCodes() {
  const [invites, setInvites] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [issuedInvite, setIssuedInvite] = useState(null);

  // The `commenter_invites_enabled` gate disables the create-invite button
  // and surfaces an inline notice. The kill switch also blocks acceptance
  // of existing un-redeemed invites — see the warning copy below.
  //
  // `null` while the fetch is in-flight or on failure. The render below
  // treats anything other than explicit `true` as "Create disabled" —
  // fail closed.
  const [invitesEnabled, setInvitesEnabled] = useState(null);

  useEffect(() => {
    fetchInvites();
    let cancelled = false;
    fetchSiteConfig()
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((data) => {
        if (!cancelled) {
          setInvitesEnabled(data.commenter_invites_enabled === true);
        }
      })
      .catch(() => {
        if (!cancelled) setInvitesEnabled(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function fetchInvites() {
    try {
      setLoading(true);
      const response = await fetchInviteCodes();
      if (!response.ok) {
        throw new Error("Failed to fetch invite codes");
      }
      const data = await response.json();
      setInvites(data);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function handleCreate({ email_hint, expires_in_hours }) {
    setError("");
    const body: { email_hint?: string; expires_in_hours?: number } = {};
    if (email_hint && email_hint.trim()) {
      body.email_hint = email_hint.trim();
    }
    if (expires_in_hours) {
      body.expires_in_hours = Number(expires_in_hours);
    }

    const response = await createInviteCode(body);

    if (!response.ok) {
      const data = await response.json();
      throw new Error(data.error || "Failed to create invite");
    }

    const data = await response.json();
    setShowCreateModal(false);
    setIssuedInvite({
      ...data,
      url: `${window.location.origin}/invite/${data.code}`,
    });
    setSuccess("Invite issued.");
    await fetchInvites();
  }

  async function handleRevoke(invite) {
    if (
      !window.confirm(
        `Revoke this invite${invite.email_hint ? ` for ${invite.email_hint}` : ""}? The recipient will no longer be able to use it.`,
      )
    ) {
      return;
    }
    setError("");
    try {
      const response = await revokeInviteCode(invite.id);
      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to revoke invite");
      }
      setSuccess("Invite revoked.");
      await fetchInvites();
    } catch (err) {
      setError(err.message);
    }
  }

  const columns = [
    {
      key: "status",
      header: "Status",
      render: (value) => (
        <span className={STATUS_BADGE[value] || "badge badge-gray"}>{value}</span>
      ),
    },
    {
      key: "email_hint",
      header: "Recipient",
      render: (value) => value || <span className="text-muted">(none)</span>,
    },
    {
      key: "created_at",
      header: "Created",
      render: (value) => formatDate(value),
    },
    {
      key: "expires_at",
      header: "Expires",
      render: (value) => formatDate(value),
    },
    {
      key: "used_at",
      header: "Used",
      render: (value, row) =>
        value ? (
          <span title={row.used_by_user_id || ""}>{formatDate(value)}</span>
        ) : (
          <span className="text-muted">—</span>
        ),
    },
    {
      key: "actions",
      header: "Actions",
      render: (_, row) =>
        row.status === "active" ? (
          <button
            type="button"
            className="btn-secondary btn-sm"
            onClick={() => handleRevoke(row)}
          >
            Revoke
          </button>
        ) : (
          <span className="text-muted">—</span>
        ),
    },
  ];

  const counts = invites.reduce(
    (acc, inv) => {
      acc.total += 1;
      acc[inv.status] = (acc[inv.status] || 0) + 1;
      return acc;
    },
    { total: 0, active: 0, used: 0, expired: 0, revoked: 0 },
  );

  return (
    <Layout>
      <div className="invite-codes-page">
        <header className="page-header">
          <h1>Invite Codes</h1>
          <button
            type="button"
            className="btn-primary"
            disabled={invitesEnabled !== true}
            title={
              invitesEnabled === true
                ? undefined
                : invitesEnabled === null
                  ? "Loading site configuration…"
                  : "Disabled in Settings — turn on commenter_invites_enabled to issue new invites"
            }
            onClick={() => {
              setError("");
              setSuccess("");
              setShowCreateModal(true);
            }}
          >
            + Create Invite
          </button>
        </header>

        {invitesEnabled === null && (
          <div className="alert alert-warning">
            Loading site configuration… invite creation is disabled until
            config loads. If this persists, check the backend.
          </div>
        )}
        {invitesEnabled === false && (
          <div className="alert alert-warning">
            Commenter invites are currently disabled in Settings. New invites
            cannot be issued AND existing un-accepted invites cannot be
            redeemed until this is turned back on. Already-activated
            commenter accounts are unaffected.
          </div>
        )}
        {error && <div className="alert alert-error">{error}</div>}
        {success && <div className="alert alert-success">{success}</div>}

        <div className="invites-stats">
          <div className="stat-card">
            <div className="stat-label">Total</div>
            <div className="stat-value">{counts.total}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Active</div>
            <div className="stat-value">{counts.active}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Used</div>
            <div className="stat-value">{counts.used}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Expired</div>
            <div className="stat-value">{counts.expired}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Revoked</div>
            <div className="stat-value">{counts.revoked}</div>
          </div>
        </div>

        <Table
          columns={columns}
          data={invites}
          loading={loading}
          emptyMessage="No invite codes yet. Create one to onboard a new user."
        />

        {showCreateModal && (
          <CreateInviteModal
            onSubmit={handleCreate}
            onClose={() => setShowCreateModal(false)}
          />
        )}

        {issuedInvite && (
          <IssuedInviteModal
            invite={issuedInvite}
            onClose={() => setIssuedInvite(null)}
          />
        )}
      </div>
    </Layout>
  );
}

function CreateInviteModal({ onSubmit, onClose }) {
  const [emailHint, setEmailHint] = useState("");
  const [expiresInHours, setExpiresInHours] = useState(168);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");
    setSubmitting(true);
    try {
      await onSubmit({
        email_hint: emailHint,
        expires_in_hours: expiresInHours,
      });
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Create Invite</h2>
          <button className="modal-close" onClick={onClose}>
            &times;
          </button>
        </div>
        <form onSubmit={handleSubmit}>
          <div className="modal-body">
            {error && <div className="alert alert-error">{error}</div>}

            <div className="form-group">
              <label htmlFor="invite-email-hint">Recipient email (optional)</label>
              <input
                id="invite-email-hint"
                type="email"
                value={emailHint}
                onChange={(e) => setEmailHint(e.target.value)}
                placeholder="friend@example.com"
              />
              <small className="form-hint">
                Used as a label and, for pre-provisioned admin/poster rows, to
                verify the IdP-attested email at bind time. Leave blank for an
                open commenter invite.
              </small>
            </div>

            <div className="form-group">
              <label htmlFor="invite-expires">Expires in (hours)</label>
              <input
                id="invite-expires"
                type="number"
                min={1}
                max={30 * 24}
                value={expiresInHours}
                onChange={(e) => setExpiresInHours(Number(e.target.value))}
              />
              <small className="form-hint">
                Default 168 (7 days). Maximum 720 (30 days).
              </small>
            </div>
          </div>
          <div className="modal-footer">
            <button
              type="button"
              className="btn-secondary"
              onClick={onClose}
              disabled={submitting}
            >
              Cancel
            </button>
            <button type="submit" className="btn-primary" disabled={submitting}>
              {submitting ? "Creating..." : "Create Invite"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function IssuedInviteModal({ invite, onClose }) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(invite.url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Best-effort; the input is selectable as a fallback.
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Invite Issued</h2>
          <button className="modal-close" onClick={onClose}>
            &times;
          </button>
        </div>
        <div className="modal-body">
          <p>
            {invite.email_hint ? (
              <>
                Invite for <strong>{invite.email_hint}</strong> has been issued.
                An email is on the way.
              </>
            ) : (
              <>Invite has been issued.</>
            )}{" "}
            Share this link with the recipient if needed. The plaintext code is
            shown <em>only once</em>.
          </p>
          <div className="form-group">
            <label htmlFor="issued-invite-link">Invite link</label>
            <input
              id="issued-invite-link"
              type="text"
              readOnly
              value={invite.url}
              onFocus={(e) => e.target.select()}
            />
          </div>
        </div>
        <div className="modal-footer">
          <button type="button" className="btn-secondary" onClick={handleCopy}>
            {copied ? "Copied!" : "Copy Link"}
          </button>
          <button type="button" className="btn-primary" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

export default InviteCodes;

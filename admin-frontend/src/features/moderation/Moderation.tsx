import { useState, useEffect, useCallback } from "react";
import Layout from "../../components/Layout";
import { Table } from "@cavebatsofware/riposte-design-system/components";
import { bulkDeleteComments, fetchModerationComments } from "./api";
import "./Moderation.css";

/// Recent comments across the whole site, with checkbox-driven bulk
/// soft-delete. Admins land here to clear spam and abuse without first
/// finding the parent post.
///
/// Bulk-delete is idempotent on the backend: re-sending an already-
/// soft-deleted id is a silent no-op and the response's `deleted` count
/// only includes rows that flipped from live to deleted in this call.
function Moderation() {
  const [rows, setRows] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");
  const [page, setPage] = useState(1);
  const [totalPages, setTotalPages] = useState(1);
  const [includeDeleted, setIncludeDeleted] = useState(false);
  const [selected, setSelected] = useState(new Set());
  const [submitting, setSubmitting] = useState(false);

  const fetchRows = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const params = new URLSearchParams({
        page: String(page),
        per_page: "50",
        include_deleted: includeDeleted ? "true" : "false",
      });
      const response = await fetchModerationComments(params.toString());
      if (!response.ok) {
        throw new Error("Failed to load comments");
      }
      const data = await response.json();
      setRows(data.data);
      setTotalPages(data.total_pages || 1);
      // Drop selections for rows that are no longer in the page.
      setSelected((prev) => {
        const next = new Set();
        for (const r of data.data) {
          if (prev.has(r.id)) next.add(r.id);
        }
        return next;
      });
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }, [page, includeDeleted]);

  useEffect(() => {
    fetchRows();
  }, [fetchRows]);

  function toggleRow(id) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  function toggleAllVisible() {
    setSelected((prev) => {
      const visibleLiveIds = rows
        .filter((r) => !r.deleted_at)
        .map((r) => r.id);
      const allChecked = visibleLiveIds.every((id) => prev.has(id));
      if (allChecked) {
        const next = new Set(prev);
        for (const id of visibleLiveIds) next.delete(id);
        return next;
      }
      const next = new Set(prev);
      for (const id of visibleLiveIds) next.add(id);
      return next;
    });
  }

  async function handleBulkDelete() {
    if (selected.size === 0) return;
    if (
      !window.confirm(
        `Soft-delete ${selected.size} comment${selected.size === 1 ? "" : "s"}? They'll be hidden from public reads but kept in the DB for audit.`,
      )
    ) {
      return;
    }
    setSubmitting(true);
    setError("");
    setSuccess("");
    try {
      const response = await bulkDeleteComments(Array.from(selected));
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to delete comments");
      }
      const data = await response.json();
      setSuccess(
        `Soft-deleted ${data.deleted} comment${data.deleted === 1 ? "" : "s"}.`,
      );
      setSelected(new Set());
      await fetchRows();
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  const visibleLiveIds = rows.filter((r) => !r.deleted_at).map((r) => r.id);
  const allVisibleSelected =
    visibleLiveIds.length > 0 && visibleLiveIds.every((id) => selected.has(id));

  const columns = [
    {
      key: "select",
      header: (
        <input
          type="checkbox"
          checked={allVisibleSelected}
          onChange={toggleAllVisible}
          aria-label="Select all visible live comments"
        />
      ),
      render: (_, row) =>
        row.deleted_at ? (
          <span className="text-muted" aria-label="Already deleted">
            
          </span>
        ) : (
          <input
            type="checkbox"
            checked={selected.has(row.id)}
            onChange={() => toggleRow(row.id)}
            aria-label={`Select comment ${row.id}`}
          />
        ),
    },
    {
      key: "body",
      header: "Comment",
      render: (value, row) => (
        <div className="moderation-comment-cell">
          <div className="moderation-comment-body">{truncate(value, 240)}</div>
          {row.deleted_at && (
            <span className="badge badge-gray">deleted</span>
          )}
        </div>
      ),
    },
    {
      key: "author_email",
      header: "Author",
      render: (value, row) => (
        <div className="moderation-author-cell">
          <div>{row.author_display || "(no display name)"}</div>
          <div className="text-muted moderation-author-email">{value}</div>
        </div>
      ),
    },
    {
      key: "post_excerpt",
      header: "On post",
      render: (value, row) => (
        <a
          href={`/post/${row.post_id}`}
          target="_blank"
          rel="noopener noreferrer"
          className="moderation-post-link"
          title="Open in social frontend"
        >
          {value || "(no excerpt)"}
        </a>
      ),
    },
    {
      key: "created_at",
      header: "Posted",
      render: (value) => formatDate(value),
    },
  ];

  return (
    <Layout>
      <div className="moderation-page">
        <header className="page-header">
          <h1>Moderation</h1>
          <label className="moderation-toggle">
            <input
              type="checkbox"
              checked={includeDeleted}
              onChange={(e) => {
                setIncludeDeleted(e.target.checked);
                setPage(1);
              }}
            />
            Show previously deleted
          </label>
        </header>

        {error && <div className="alert alert-error">{error}</div>}
        {success && <div className="alert alert-success">{success}</div>}

        <div className="moderation-toolbar">
          <span className="moderation-toolbar-info">
            {selected.size > 0
              ? `${selected.size} selected`
              : "Select rows to bulk-delete"}
          </span>
          <button
            type="button"
            className="btn-danger"
            disabled={selected.size === 0 || submitting}
            onClick={handleBulkDelete}
          >
            {submitting ? "Deleting…" : "Delete selected"}
          </button>
        </div>

        <Table
          columns={columns}
          data={rows}
          loading={loading}
          emptyMessage={
            includeDeleted
              ? "No comments found."
              : "No live comments to moderate."
          }
          pagination={{
            page,
            totalPages,
            onPageChange: (p) => {
              setPage(p);
            },
          }}
        />
      </div>
    </Layout>
  );
}

function truncate(s, n) {
  if (!s) return "";
  if (s.length <= n) return s;
  return s.slice(0, n) + "...";
}

function formatDate(value) {
  if (!value) return "";
  try {
    return new Date(value).toLocaleString();
  } catch {
    return value;
  }
}

export default Moderation;

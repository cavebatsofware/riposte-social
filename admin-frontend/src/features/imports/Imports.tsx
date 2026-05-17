import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Layout from "../../components/Layout";
import Table from "../../components/Table";
import {
  fetchImport,
  fetchImports,
  fetchSiteConfig,
  startFacebookImport,
} from "./api";
import "./Imports.css";

/// Admin page at `/imports`. Drives the homegrown import job runner:
/// drag-drop a Facebook archive ZIP, pick the visibility tier the imported
/// posts should land at, kick off the worker, and watch progress. Below
/// the form is a list of recent jobs; clicking one opens a detail panel
/// that polls `GET /api/admin/imports/{id}` and renders the structured
/// per-job log so the operator can see exactly what happened on each run.
///
/// Job status flow (from `src/imports/mod.rs`):
///   queued -> running -> succeeded | failed | cancelled
///
/// We poll while a job is non-terminal. Terminal jobs are static; clicking
/// a different one switches the active polling target.
const VISIBILITIES = [
  { id: "public", label: "Public" },
  { id: "commenters", label: "Commenters" },
  { id: "posters", label: "Posters" },
];

const TERMINAL_STATUSES = new Set(["succeeded", "failed", "cancelled"]);

function Imports() {
  const [jobs, setJobs] = useState([]);
  const [jobsLoading, setJobsLoading] = useState(true);
  const [jobsError, setJobsError] = useState("");
  const [selectedId, setSelectedId] = useState(null);
  const [selectedJob, setSelectedJob] = useState(null);

  const [file, setFile] = useState(null);
  const [visibility, setVisibility] = useState("public");
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState("");
  const fileInputRef = useRef(null);

  // The `fb_import_enabled` gate hides the upload form when an admin has
  // muted imports site-wide. The list view still works (admins can review
  // historical jobs while imports are paused).
  //
  // `null` while the fetch is in-flight or on failure. The render below
  // treats anything other than explicit `true` as "form disabled"  fail
  // closed so a transient settings outage never shows a form that the
  // backend will then reject.
  const [importsEnabled, setImportsEnabled] = useState(null);
  useEffect(() => {
    let cancelled = false;
    fetchSiteConfig()
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((data) => {
        if (!cancelled) {
          setImportsEnabled(data.fb_import_enabled === true);
        }
      })
      .catch(() => {
        if (!cancelled) setImportsEnabled(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const fetchJobs = useCallback(async () => {
    try {
      const response = await fetchImports(25);
      if (!response.ok) throw new Error("Failed to load imports");
      const data = await response.json();
      setJobs(data.data || []);
    } catch (e) {
      setJobsError(e.message);
    } finally {
      setJobsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchJobs();
  }, [fetchJobs]);

  // Poll the selected job while it's non-terminal. Switching the selected
  // id (or hitting a terminal state) tears the interval down.
  useEffect(() => {
    if (!selectedId) {
      setSelectedJob(null);
      return undefined;
    }
    let cancelled = false;
    let timer = null;
    async function tick() {
      try {
        const response = await fetchImport(selectedId);
        if (!response.ok) throw new Error("Failed to load job");
        const data = await response.json();
        if (cancelled) return;
        setSelectedJob(data);
        if (TERMINAL_STATUSES.has(data.status)) {
          // Refresh the list one last time so the row's status reflects
          // the terminal state, then stop polling.
          fetchJobs();
          return;
        }
        timer = setTimeout(tick, 1500);
      } catch (e) {
        if (!cancelled) {
          setSelectedJob((prev) =>
            prev ? { ...prev, _pollError: e.message } : null,
          );
          timer = setTimeout(tick, 3000);
        }
      }
    }
    tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [selectedId, fetchJobs]);

  function onPickFile(e) {
    const f = e.target.files?.[0];
    if (f) setFile(f);
    e.target.value = "";
  }

  function onDrop(e) {
    e.preventDefault();
    const f = e.dataTransfer.files?.[0];
    if (f) setFile(f);
  }

  async function onSubmit(e) {
    e.preventDefault();
    if (!file || submitting) return;
    setSubmitting(true);
    setSubmitError("");
    try {
      const form = new FormData();
      form.append("archive", file);
      form.append("visibility", visibility);
      const response = await startFacebookImport(form);
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.error || "Failed to start import");
      }
      const data = await response.json();
      setFile(null);
      await fetchJobs();
      setSelectedId(data.id);
    } catch (err) {
      setSubmitError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  const columns = useMemo(
    () => [
      {
        key: "kind",
        header: "Kind",
        render: (v) => v,
      },
      {
        key: "status",
        header: "Status",
        render: (v) => <StatusBadge status={v} />,
      },
      {
        key: "progress",
        header: "Progress",
        render: (_, row) => <ProgressCell row={row} />,
      },
      {
        key: "created_at",
        header: "Started",
        render: (v) => new Date(v).toLocaleString(),
      },
      {
        key: "_actions",
        header: "",
        render: (_, row) => (
          <button
            type="button"
            className="btn-secondary btn-sm"
            onClick={() => setSelectedId(row.id)}
          >
            {selectedId === row.id ? "Viewing" : "View"}
          </button>
        ),
      },
    ],
    [selectedId],
  );

  return (
    <Layout>
      <div className="imports-page">
        <header className="page-header">
          <h1>Imports</h1>
        </header>

        <section className="imports-form-section">
          <h2>New Facebook archive</h2>
          {importsEnabled === null && (
            <div className="alert alert-warning">
              Loading site configuration… the form is disabled until config
              loads. If this persists, check the backend.
            </div>
          )}
          {importsEnabled === false && (
            <div className="alert alert-warning">
              Facebook imports are currently disabled in Settings. Existing
              jobs are unaffected.
            </div>
          )}
          {submitError && <div className="alert alert-error">{submitError}</div>}
          <form
            className="imports-form"
            onSubmit={onSubmit}
            style={
              importsEnabled === true
                ? undefined
                : { opacity: 0.5, pointerEvents: "none" }
            }
          >
            <div
              className={`imports-dropzone ${file ? "has-file" : ""}`}
              onDragOver={(e) => e.preventDefault()}
              onDrop={onDrop}
              onClick={() => fileInputRef.current?.click()}
              role="button"
              tabIndex={0}
            >
              {file ? (
                <>
                  <strong>{file.name}</strong>
                  <span className="text-muted">
                    {formatBytes(file.size)}
                  </span>
                </>
              ) : (
                <>
                  <p>Drop a Facebook export ZIP here, or click to browse.</p>
                  <p className="text-muted">
                    The archive is uploaded once; the job runs in the
                    background. The table below shows live progress.
                  </p>
                </>
              )}
              <input
                ref={fileInputRef}
                type="file"
                accept=".zip,application/zip"
                style={{ display: "none" }}
                onChange={onPickFile}
              />
            </div>

            <fieldset className="imports-visibility">
              <legend>Visibility for imported posts</legend>
              {VISIBILITIES.map((v) => (
                <label key={v.id} className="imports-visibility-option">
                  <input
                    type="radio"
                    name="visibility"
                    value={v.id}
                    checked={visibility === v.id}
                    onChange={() => setVisibility(v.id)}
                  />
                  {v.label}
                </label>
              ))}
            </fieldset>

            <div className="imports-form-actions">
              <button
                type="submit"
                className="btn-primary"
                disabled={!file || submitting}
              >
                {submitting ? "Uploading…" : "Start import"}
              </button>
              {file && (
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={() => setFile(null)}
                  disabled={submitting}
                >
                  Clear
                </button>
              )}
            </div>
          </form>
        </section>

        <section className="imports-list-section">
          <h2>Recent jobs</h2>
          {jobsError && <div className="alert alert-error">{jobsError}</div>}
          <Table
            columns={columns}
            data={jobs}
            loading={jobsLoading}
            emptyMessage="No imports yet."
          />
        </section>

        {selectedJob && (
          <JobDetailPanel
            job={selectedJob}
            onClose={() => setSelectedId(null)}
          />
        )}
      </div>
    </Layout>
  );
}

function ProgressCell({ row }) {
  const p = row.progress || {};
  const total = p.total || 0;
  const processed = p.processed || 0;
  const pct = total > 0 ? Math.min(100, Math.round((processed / total) * 100)) : 0;
  if (TERMINAL_STATUSES.has(row.status)) {
    return (
      <span>
        {p.succeeded || 0} ok · {p.skipped || 0} skip · {p.failed || 0} fail
      </span>
    );
  }
  return (
    <div className="imports-progress-cell">
      <div className="imports-progress-bar">
        <div className="imports-progress-fill" style={{ width: `${pct}%` }} />
      </div>
      <span className="imports-progress-label">
        {processed}/{total}
      </span>
    </div>
  );
}

function StatusBadge({ status }) {
  const cls =
    {
      queued: "badge badge-gray",
      running: "badge badge-info",
      succeeded: "badge badge-success",
      failed: "badge badge-danger",
      cancelled: "badge badge-warning",
    }[status] || "badge badge-gray";
  return <span className={cls}>{status}</span>;
}

function JobDetailPanel({ job, onClose }) {
  const log = job.log || { entries: [], dropped: 0 };
  const entries = log.entries || [];
  return (
    <section className="imports-detail">
      <header className="imports-detail-header">
        <div>
          <h2>Job {shortId(job.id)}</h2>
          <p className="text-muted">
            <StatusBadge status={job.status} />
            {job.params?.original_filename && (
              <>
                {" · "}
                {job.params.original_filename}
              </>
            )}
            {job.params?.size_bytes && (
              <>
                {" · "}
                {formatBytes(job.params.size_bytes)}
              </>
            )}
          </p>
        </div>
        <button type="button" className="btn-secondary" onClick={onClose}>
          Close
        </button>
      </header>

      <div className="imports-detail-grid">
        <div>
          <h3>Progress</h3>
          <dl className="imports-detail-counters">
            <div>
              <dt>Total</dt>
              <dd>{job.progress?.total ?? 0}</dd>
            </div>
            <div>
              <dt>Processed</dt>
              <dd>{job.progress?.processed ?? 0}</dd>
            </div>
            <div>
              <dt>Succeeded</dt>
              <dd>{job.progress?.succeeded ?? 0}</dd>
            </div>
            <div>
              <dt>Skipped</dt>
              <dd>{job.progress?.skipped ?? 0}</dd>
            </div>
            <div>
              <dt>Failed</dt>
              <dd>{job.progress?.failed ?? 0}</dd>
            </div>
          </dl>
          {job.error && (
            <>
              <h3>Final error</h3>
              <pre className="imports-detail-error">{job.error}</pre>
            </>
          )}
          {job._pollError && (
            <p className="text-muted">Poll error: {job._pollError}</p>
          )}
        </div>
        <div>
          <h3>Log</h3>
          {log.dropped > 0 && (
            <p className="text-muted">
              {log.dropped} earlier entries omitted (buffer full)
            </p>
          )}
          {entries.length === 0 ? (
            <p className="text-muted">No entries yet.</p>
          ) : (
            <ol className="imports-log">
              {[...entries].reverse().map((e, i) => (
                <li
                  key={`${e.ts}-${i}`}
                  className={`imports-log-entry imports-log-${e.level}`}
                >
                  <span className="imports-log-ts">
                    {new Date(e.ts).toLocaleTimeString()}
                  </span>
                  <span className="imports-log-level">{e.level}</span>
                  <span className="imports-log-msg">{e.msg}</span>
                  {e.ctx && (
                    <pre className="imports-log-ctx">
                      {JSON.stringify(e.ctx, null, 2)}
                    </pre>
                  )}
                </li>
              ))}
            </ol>
          )}
        </div>
      </div>
    </section>
  );
}

function shortId(id) {
  return id ? id.slice(0, 8) : "";
}

function formatBytes(n) {
  if (n == null) return "";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export default Imports;

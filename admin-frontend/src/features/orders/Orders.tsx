import React, { useState, useEffect } from "react";
import Layout from "../../components/Layout";
import { Table } from "@cavebatsofware/riposte-design-system/components";
import {
  fetchOrders as fetchOrdersApi,
  fetchOrderStatuses,
  updateOrderStatus,
} from "./api";
import "./Orders.css";

function formatDate(dateString) {
  if (!dateString) return "N/A";
  const date = new Date(dateString);
  return date.toLocaleDateString() + " " + date.toLocaleTimeString();
}

function formatMoney(value) {
  if (value === null || value === undefined) return "-";
  return "$" + Number(value).toFixed(2);
}

function Orders() {
  const [orders, setOrders] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [currentPage, setCurrentPage] = useState(1);
  const [totalPages, setTotalPages] = useState(1);
  const [total, setTotal] = useState(0);
  const [perPage] = useState(50);
  const [statuses, setStatuses] = useState([]);
  const [statusFilter, setStatusFilter] = useState("");
  const [selected, setSelected] = useState(null);

  useEffect(() => {
    fetchOrders(currentPage);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentPage, statusFilter]);

  useEffect(() => {
    fetchOrderStatuses()
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => {
        if (d?.statuses) setStatuses(d.statuses);
      })
      .catch(() => {});
  }, []);

  async function fetchOrders(page = 1) {
    try {
      setLoading(true);
      const response = await fetchOrdersApi(page, perPage, statusFilter);
      if (!response.ok) {
        throw new Error("Failed to fetch orders");
      }
      const data = await response.json();
      setOrders(data.data);
      setTotal(data.total);
      setTotalPages(data.total_pages);
      setCurrentPage(data.page);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function changeStatus(id, status) {
    try {
      const response = await updateOrderStatus(id, status);
      if (!response.ok) {
        throw new Error("Failed to update status");
      }
      setOrders((prev) =>
        prev.map((o) => (o.id === id ? { ...o, status } : o)),
      );
      setSelected((s) => (s && s.id === id ? { ...s, status } : s));
    } catch (err) {
      setError(err.message);
    }
  }

  function handlePageChange(newPage) {
    if (newPage >= 1 && newPage <= totalPages) {
      setCurrentPage(newPage);
    }
  }

  function onFilterChange(value) {
    setStatusFilter(value);
    setCurrentPage(1);
  }

  const columns = [
    {
      key: "created_at",
      header: "Received",
      render: (value) => formatDate(value),
    },
    {
      key: "title",
      header: "Order",
      render: (value, row) => <span title={row.summary}>{value}</span>,
    },
    {
      key: "customer_name",
      header: "Customer",
    },
    {
      key: "customer_phone",
      header: "Phone",
      render: (value) => <a href={`tel:${value}`}>{value}</a>,
    },
    {
      key: "estimate_total",
      header: "Estimate",
      render: (value) => formatMoney(value),
    },
    {
      key: "status",
      header: "Status",
      render: (value, row) => {
        const options = statuses.includes(value)
          ? statuses
          : [value, ...statuses];
        return (
          <select
            className="order-status-select"
            value={value}
            onChange={(e) => changeStatus(row.id, e.target.value)}
          >
            {options.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        );
      },
    },
    {
      key: "id",
      header: "",
      render: (_value, row) => (
        <button
          type="button"
          className="btn-secondary btn-sm"
          onClick={() => setSelected(row)}
        >
          View
        </button>
      ),
    },
  ];

  return (
    <Layout>
      <div className="orders-page">
        <header className="page-header">
          <h1>Orders</h1>
          <label className="orders-filter">
            Status:{" "}
            <select
              value={statusFilter}
              onChange={(e) => onFilterChange(e.target.value)}
            >
              <option value="">All</option>
              {statuses.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
          </label>
        </header>

        {error && <div className="error">{error}</div>}

        <div className="orders-stats">
          <div className="stat-card">
            <div className="stat-label">Total Orders</div>
            <div className="stat-value">{total}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Current Page</div>
            <div className="stat-value">
              {currentPage} / {totalPages}
            </div>
          </div>
        </div>

        <Table
          columns={columns}
          data={orders}
          loading={loading}
          emptyMessage="No orders yet."
          pagination={{
            page: currentPage,
            totalPages,
            onPageChange: handlePageChange,
          }}
        />
      </div>

      {selected && (
        <OrderDetailModal
          order={selected}
          statuses={statuses}
          onChangeStatus={changeStatus}
          onClose={() => setSelected(null)}
        />
      )}
    </Layout>
  );
}

function OrderDetailModal({ order, statuses, onChangeStatus, onClose }) {
  useEffect(() => {
    function onKey(e) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  let details = {};
  try {
    details = JSON.parse(order.details || "{}");
  } catch {
    details = {};
  }
  const detailEntries = Object.entries(details);
  const statusOptions = statuses.includes(order.status)
    ? statuses
    : [order.status, ...statuses];

  return (
    <div
      className="order-modal-overlay"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="order-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Order detail"
      >
        <header className="order-modal-head">
          <h2>{order.title}</h2>
          <button
            type="button"
            className="order-modal-close"
            aria-label="Close"
            onClick={onClose}
          >
            ×
          </button>
        </header>

        <div className="order-modal-body">
          <div className="order-modal-row">
            <span className="order-modal-label">Status</span>
            <select
              className="order-status-select"
              value={order.status}
              onChange={(e) => onChangeStatus(order.id, e.target.value)}
            >
              {statusOptions.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
          </div>
          <div className="order-modal-row">
            <span className="order-modal-label">Received</span>
            <span>{formatDate(order.created_at)}</span>
          </div>
          <div className="order-modal-row">
            <span className="order-modal-label">Customer</span>
            <span>{order.customer_name}</span>
          </div>
          <div className="order-modal-row">
            <span className="order-modal-label">Phone</span>
            <span>
              <a href={`tel:${order.customer_phone}`}>{order.customer_phone}</a>
            </span>
          </div>
          {order.phone_verified != null && (
            <div className="order-modal-row">
              <span className="order-modal-label">Phone check</span>
              <span>
                {order.phone_verified
                  ? `verified${order.phone_line_type ? ` (${order.phone_line_type})` : ""}`
                  : "invalid"}
              </span>
            </div>
          )}
          {order.customer_email && (
            <div className="order-modal-row">
              <span className="order-modal-label">Email</span>
              <span>{order.customer_email}</span>
            </div>
          )}
          <div className="order-modal-row">
            <span className="order-modal-label">Estimate</span>
            <span>{formatMoney(order.estimate_total)}</span>
          </div>
          {(order.total !== null && order.total !== undefined) && (
            <div className="order-modal-row">
              <span className="order-modal-label">Total</span>
              <span>{formatMoney(order.total)}</span>
            </div>
          )}
          {(order.deposit !== null && order.deposit !== undefined) && (
            <div className="order-modal-row">
              <span className="order-modal-label">Deposit</span>
              <span>{formatMoney(order.deposit)}</span>
            </div>
          )}

          <h3 className="order-modal-subhead">Summary</h3>
          <pre className="order-modal-summary">{order.summary}</pre>

          {detailEntries.length > 0 && (
            <>
              <h3 className="order-modal-subhead">Build details</h3>
              <dl className="order-modal-details">
                {detailEntries.map(([k, v]) => (
                  <React.Fragment key={k}>
                    <dt>{k}</dt>
                    <dd>{String(v)}</dd>
                  </React.Fragment>
                ))}
              </dl>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export default Orders;

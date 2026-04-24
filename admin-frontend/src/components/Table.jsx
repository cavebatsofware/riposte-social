import React from "react";
import "./Table.css";

/**
 * Reusable Table Component
 *
 * @param {Array} columns - Column definitions [{ key, header, render? }]
 * @param {Array} data - Array of row data objects
 * @param {Function} getRowKey - Function to extract unique key from row (default: row => row.id)
 * @param {Function} getRowClassName - Optional function to add classes to rows (row) => string
 * @param {boolean} loading - Show loading state
 * @param {string} emptyMessage - Message when no data
 * @param {Object} pagination - Pagination config { page, totalPages, onPageChange }
 */
function Table({
  columns,
  data,
  getRowKey = (row) => row.id,
  getRowClassName,
  loading = false,
  emptyMessage = "No data available",
  pagination,
}) {
  if (loading) {
    return <div className="loading">Loading...</div>;
  }

  if (!data || data.length === 0) {
    return (
      <div className="empty-state">
        <p>{emptyMessage}</p>
      </div>
    );
  }

  return (
    <div className="table-container">
      <table className="data-table">
        <thead>
          <tr>
            {columns.map((col) => (
              <th key={col.key}>{col.header}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.map((row) => (
            <tr
              key={getRowKey(row)}
              className={getRowClassName ? getRowClassName(row) : ""}
            >
              {columns.map((col) => (
                <td key={col.key}>
                  {col.render ? col.render(row[col.key], row) : row[col.key]}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>

      {pagination && pagination.totalPages > 1 && (
        <div className="pagination">
          <button
            onClick={() => pagination.onPageChange(1)}
            disabled={pagination.page === 1}
            className="pagination-btn"
          >
            First
          </button>
          <button
            onClick={() => pagination.onPageChange(pagination.page - 1)}
            disabled={pagination.page === 1}
            className="pagination-btn"
          >
            Previous
          </button>
          <span className="pagination-info">
            Page {pagination.page} of {pagination.totalPages}
          </span>
          <button
            onClick={() => pagination.onPageChange(pagination.page + 1)}
            disabled={pagination.page === pagination.totalPages}
            className="pagination-btn"
          >
            Next
          </button>
          <button
            onClick={() => pagination.onPageChange(pagination.totalPages)}
            disabled={pagination.page === pagination.totalPages}
            className="pagination-btn"
          >
            Last
          </button>
        </div>
      )}
    </div>
  );
}

export default Table;

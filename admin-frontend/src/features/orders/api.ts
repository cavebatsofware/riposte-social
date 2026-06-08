import { fetchApi } from "../../utils/api";

export function fetchOrders(page = 1, perPage = 50, status = "") {
  const q = new URLSearchParams({
    page: String(page),
    per_page: String(perPage),
  });
  if (status) q.set("status", status);
  return fetchApi(`/api/admin/orders?${q.toString()}`);
}

export function fetchOrderStatuses() {
  return fetchApi("/api/admin/orders/statuses");
}

export function updateOrderStatus(id, status) {
  return fetchApi(`/api/admin/orders/${id}/status`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ status }),
  });
}

import { fetchApi } from "../../utils/api";

export function fetchAccessLogs(page = 1, perPage = 25) {
  return fetchApi(`/api/admin/access-logs?page=${page}&per_page=${perPage}`);
}

export function clearAccessLogs() {
  return fetchApi("/api/admin/access-logs", { method: "DELETE" });
}

import { fetchApi } from "../../utils/api";

export function fetchDashboardMetrics() {
  return fetchApi("/api/admin/dashboard/metrics");
}

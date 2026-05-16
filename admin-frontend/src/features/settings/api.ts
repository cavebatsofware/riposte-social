import { fetchApi } from "../../utils/api";

export function fetchSettings() {
  return fetchApi("/api/admin/settings");
}

export function updateSetting(body) {
  return fetchApi("/api/admin/settings", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

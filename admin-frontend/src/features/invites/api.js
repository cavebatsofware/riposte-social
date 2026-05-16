import { fetchApi } from "../../utils/api";

export function fetchSiteConfig() {
  return fetchApi("/api/site/config");
}

export function fetchInviteCodes() {
  return fetchApi("/api/admin/invite-codes");
}

export function createInviteCode(body) {
  return fetchApi("/api/admin/invite-codes", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function revokeInviteCode(id) {
  return fetchApi(`/api/admin/invite-codes/${id}`, { method: "DELETE" });
}

import { fetchApi } from "../../utils/api";

// Invite splash + acceptance flow
export function fetchCurrentInvite() {
  return fetchApi("/api/invites/current");
}

export function confirmInvite(code) {
  return fetchApi("/api/invites/confirm", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code }),
  });
}

export function clearPendingInvite() {
  return fetchApi("/api/auth/logout/invite", { method: "POST" });
}

export function acceptInvitePassword(body) {
  return fetchApi("/api/auth/invite/accept-password", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

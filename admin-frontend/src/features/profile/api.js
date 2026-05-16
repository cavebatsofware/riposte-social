import { fetchApi } from "../../utils/api";

export function changeMyPassword(body) {
  return fetchApi("/api/me/password", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function startMfaSetup() {
  return fetchApi("/api/me/mfa/setup", { method: "POST" });
}

export function confirmMfaSetup(body) {
  return fetchApi("/api/me/mfa/confirm-setup", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function disableMfa(body) {
  return fetchApi("/api/me/mfa/disable", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

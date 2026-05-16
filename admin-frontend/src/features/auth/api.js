import { fetchApi } from "../../utils/api";

export function resetPassword(body) {
  return fetchApi("/api/auth/reset-password", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function changeMyPassword(body) {
  return fetchApi("/api/me/password", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function forgotPassword(body) {
  return fetchApi("/api/auth/forgot-password", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function forgotPasswordVerifyMfa(body) {
  return fetchApi("/api/auth/forgot-password/verify-mfa", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function verifyEmail(token) {
  return fetchApi(`/api/auth/verify-email?token=${token}`, { method: "GET" });
}

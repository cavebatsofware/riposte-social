import { fetchApi } from "../../utils/api";

export function fetchAdminUsers(page = 1, perPage = 20) {
  return fetchApi(`/api/admin/users?page=${page}&per_page=${perPage}`);
}

export function updateAdminUser(userId, updates) {
  return fetchApi(`/api/admin/users/${userId}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(updates),
  });
}

export function createAdminUser(body) {
  return fetchApi("/api/admin/users", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function resendUserVerification(userId) {
  return fetchApi(`/api/admin/users/${userId}/resend-verification`, {
    method: "POST",
  });
}

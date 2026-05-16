import { fetchApi } from "../../utils/api";

// Public profile + author feed
export function fetchProfile(handle) {
  return fetchApi(`/api/profiles/${encodeURIComponent(handle)}`);
}

export function fetchAuthorFeed(queryString) {
  return fetchApi(`/api/feed?${queryString}`);
}

// People directory + per-user followers / following
export function fetchProfilesDirectory(limit = 200) {
  return fetchApi(`/api/profiles?limit=${limit}`);
}

export function fetchUserConnections(userId, variant, queryString) {
  const suffix = queryString ? `?${queryString}` : "?limit=50";
  return fetchApi(`/api/users/${userId}/${variant}${suffix}`);
}

// Follow / unfollow
export function toggleFollow(userId, follow) {
  return fetchApi(`/api/users/${userId}/follow`, {
    method: follow ? "POST" : "DELETE",
  });
}

// Self-service profile edits
export function fetchMyProfile() {
  return fetchApi("/api/me/profile");
}

export function updateMyProfile(body) {
  return fetchApi("/api/me/profile", {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function uploadAvatar(formData) {
  return fetchApi("/api/me/avatar", { method: "POST", body: formData });
}

export function deleteAvatar() {
  return fetchApi("/api/me/avatar", { method: "DELETE" });
}

// Security settings: password + MFA
export function changePassword(body) {
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

export function updateLocale(locale) {
  return fetchApi("/api/me/locale", {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ locale }),
  });
}

import { fetchApi } from "../../utils/api";

// Comment endpoints. `baseUrl` differs between post-level
// (`/api/posts/{id}/comments`) and per-media
// (`/api/posts/{postId}/media/{mediaId}/comments`); the same wire shapes
// apply to both so the caller passes the base.
export function fetchComments(baseUrl) {
  return fetchApi(baseUrl);
}

export function createComment(baseUrl, body) {
  return fetchApi(baseUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ body }),
  });
}

export function editComment(baseUrl, commentId, body) {
  return fetchApi(`${baseUrl}/${commentId}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ body }),
  });
}

export function deleteComment(baseUrl, commentId) {
  return fetchApi(`${baseUrl}/${commentId}`, { method: "DELETE" });
}

// Reaction endpoints. `baseUrl` resolves the same way as for comments:
// post-level, per-comment, or per-media.
export function addReaction(baseUrl, kind) {
  return fetchApi(baseUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ kind }),
  });
}

export function removeReaction(baseUrl, kind) {
  return fetchApi(`${baseUrl}/${kind}`, { method: "DELETE" });
}

// Lightbox's lazy-loaded per-media engagement summary.
export function fetchMediaEngagement(postId, mediaId) {
  return fetchApi(`/api/posts/${postId}/media/${mediaId}/engagement`);
}

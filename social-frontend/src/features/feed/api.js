import { fetchApi } from "../../utils/api";

export function fetchFeed(queryString) {
  return fetchApi(`/api/feed?${queryString}`);
}

export function fetchPost(id) {
  return fetchApi(`/api/posts/${id}`);
}

export function deletePost(id) {
  return fetchApi(`/api/posts/${id}`, { method: "DELETE" });
}

export function patchPostVisibility(id, visibility) {
  return fetchApi(`/api/posts/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ visibility }),
  });
}

import { fetchApi } from "../../utils/api";

export function fetchCategoriesForCompose() {
  return fetchApi("/api/categories");
}

export function fetchPostForEdit(id) {
  return fetchApi(`/api/posts/${id}`);
}

export function fetchAlbumForEdit(id) {
  return fetchApi(`/api/albums/${id}`);
}

export function createPost(formData) {
  return fetchApi("/api/posts", { method: "POST", body: formData });
}

export function updatePost(id, body) {
  return fetchApi(`/api/posts/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function createAlbum(formData) {
  return fetchApi("/api/albums", { method: "POST", body: formData });
}

export function updateAlbum(id, body) {
  return fetchApi(`/api/albums/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function appendAlbumMedia(id, formData) {
  return fetchApi(`/api/albums/${id}/media`, { method: "POST", body: formData });
}

export function updateAlbumMediaCaption(albumId, mediaId, caption) {
  return fetchApi(`/api/albums/${albumId}/media/${mediaId}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ caption }),
  });
}

export function deleteAlbumMedia(albumId, mediaId) {
  return fetchApi(`/api/albums/${albumId}/media/${mediaId}`, { method: "DELETE" });
}

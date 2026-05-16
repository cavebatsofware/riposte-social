import { fetchApi } from "../../utils/api";

export function fetchAlbum(id) {
  return fetchApi(`/api/albums/${id}`);
}

export function fetchAlbums(queryString = "limit=100") {
  return fetchApi(`/api/albums?${queryString}`);
}

export function deleteAlbum(id) {
  return fetchApi(`/api/albums/${id}`, { method: "DELETE" });
}

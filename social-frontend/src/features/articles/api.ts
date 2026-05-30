import { fetchApi } from "../../utils/api";

export function fetchArticles(queryString) {
  const qs = queryString ? `?${queryString}` : "";
  return fetchApi(`/api/articles${qs}`);
}

export function fetchArticle(id) {
  return fetchApi(`/api/articles/${id}`);
}

export function fetchUserArticles(userId, queryString) {
  const qs = queryString ? `?${queryString}` : "";
  return fetchApi(`/api/users/${userId}/articles${qs}`);
}

export function fetchMyDrafts() {
  return fetchApi(`/api/articles/drafts`);
}

export function createArticle(body) {
  return fetchApi(`/api/articles`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function updateArticle(id, patch) {
  return fetchApi(`/api/articles/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(patch),
  });
}

export function deleteArticle(id) {
  return fetchApi(`/api/articles/${id}`, { method: "DELETE" });
}

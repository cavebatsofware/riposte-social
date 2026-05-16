import { fetchApi } from "../../utils/api";

export function fetchCategories() {
  return fetchApi("/api/categories");
}

export function createCategory(body) {
  return fetchApi("/api/categories", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function updateCategory(id, body) {
  return fetchApi(`/api/categories/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function deleteCategory(id) {
  return fetchApi(`/api/categories/${id}`, { method: "DELETE" });
}

export function fetchCategoryMembers(id) {
  return fetchApi(`/api/categories/${id}/members`);
}

export function fetchProfilesForPicker(limit = 200) {
  return fetchApi(`/api/profiles?limit=${limit}`);
}

export function replaceCategoryMembers(id, userIds) {
  return fetchApi(`/api/categories/${id}/members`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ user_ids: userIds }),
  });
}

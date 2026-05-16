import { fetchApi } from "../../utils/api";

export function fetchAccessCodes() {
  return fetchApi("/api/admin/access-codes");
}

export function createAccessCode(formData) {
  return fetchApi("/api/admin/access-codes", {
    method: "POST",
    body: formData,
  });
}

export function deleteAccessCode(id) {
  return fetchApi(`/api/admin/access-codes/${id}`, { method: "DELETE" });
}

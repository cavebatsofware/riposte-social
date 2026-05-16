import { fetchApi } from "../../utils/api";

export function fetchSiteConfig() {
  return fetchApi("/api/site/config");
}

export function fetchImports(perPage = 25) {
  return fetchApi(`/api/admin/imports?per_page=${perPage}`);
}

export function fetchImport(id) {
  return fetchApi(`/api/admin/imports/${id}`);
}

export function startFacebookImport(formData) {
  return fetchApi("/api/admin/imports/facebook", {
    method: "POST",
    body: formData,
  });
}

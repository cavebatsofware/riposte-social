import { fetchApi } from "../../utils/api";

export function fetchModerationComments(queryString) {
  return fetchApi(`/api/admin/moderation/comments?${queryString}`);
}

export function bulkDeleteComments(commentIds) {
  return fetchApi("/api/admin/moderation/comments/bulk-delete", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ comment_ids: commentIds }),
  });
}

/// Cypress helpers for talking to the test app's API directly. Used by
/// feature-test specs that need to seed posts / albums without going
/// through the (multipart-heavy) compose UI.
///
/// Why a hand-rolled multipart body? `cy.request` doesn't natively
/// serialize `FormData`, and pulling browser-side `fetch` into the test
/// just to use FormData is more moving parts than the simple text-only
/// payloads here actually need. The text-only multipart we build here
/// is what the Rust handlers expect (`body=`, `name=`, etc.) — no media
/// fields, since the CI test env intentionally has no S3 access.

function multipartBody(fields, boundary) {
  let body = "";
  for (const [name, value] of Object.entries(fields)) {
    body += `--${boundary}\r\n`;
    body += `Content-Disposition: form-data; name="${name}"\r\n\r\n`;
    body += `${value}\r\n`;
  }
  body += `--${boundary}--\r\n`;
  return body;
}

function postMultipart(url, fields) {
  return cy
    .request("GET", "/api/auth/csrf-token")
    .then(({ body: { token } }) => {
      const boundary = "----rs-test-boundary";
      return cy.request({
        method: "POST",
        url,
        headers: {
          "x-csrf-token": token,
          "content-type": `multipart/form-data; boundary=${boundary}`,
        },
        body: multipartBody(fields, boundary),
      });
    });
}

/// Create a text-only post via `/api/posts`. Caller must already be
/// logged in (`cy.login()`). Default visibility is `public`.
Cypress.Commands.add("createPost", (fields) => {
  return postMultipart("/api/posts", { visibility: "public", ...fields });
});

/// Create a text-only album via `/api/albums`. `name` is required by the
/// backend; `description` is optional. No media — that's a follow-up
/// concern when the CI env grows a MinIO sidecar.
Cypress.Commands.add("createAlbum", (fields) => {
  return postMultipart("/api/albums", { visibility: "public", ...fields });
});

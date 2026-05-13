// Feature spec: confirm cross-kind URL hits return 404 from the API.
//
// `/api/posts/{album_id}` and `/api/albums/{post_id}` both filter on
// `kind` in the WHERE clause, so a wrong-kind row produces the same
// NotFound surface as a missing row. That's the existence-non-disclosure
// guarantee: a probe can't distinguish "doesn't exist" from "exists as
// the other kind".
//
// S3-free: text-only fixtures so the spec runs without an object store.

describe("post / album URL kind guard", () => {
  it("GET /api/posts/{album_id} returns 404", () => {
    cy.login();
    cy.createAlbum({ name: "kind-routing album" }).then(({ body }) => {
      cy.request({
        method: "GET",
        url: `/api/posts/${body.id}`,
        failOnStatusCode: false,
      }).then(({ status }) => {
        expect(status).to.eq(404);
      });
    });
  });

  it("GET /api/albums/{post_id} returns 404", () => {
    cy.login();
    cy.createPost({ body: "kind-routing post" }).then(({ body }) => {
      cy.request({
        method: "GET",
        url: `/api/albums/${body.id}`,
        failOnStatusCode: false,
      }).then(({ status }) => {
        expect(status).to.eq(404);
      });
    });
  });

  it("DELETE /api/posts/{album_id} returns 404 (no soft-delete leak across kinds)", () => {
    cy.login();
    cy.request("GET", "/api/auth/csrf-token").then(({ body: { token } }) => {
      cy.createAlbum({ name: "kind-routing del album" }).then(({ body }) => {
        cy.request({
          method: "DELETE",
          url: `/api/posts/${body.id}`,
          headers: { "x-csrf-token": token },
          failOnStatusCode: false,
        }).then(({ status }) => {
          expect(status).to.eq(404);
        });

        // Verify the album is still readable from its own surface (the
        // DELETE on the wrong kind didn't accidentally soft-delete it).
        cy.request("GET", `/api/albums/${body.id}`)
          .its("status")
          .should("eq", 200);
      });
    });
  });
});

// Feature spec: confirm the unified posts/albums table's `kind`
// discriminator is honored by the feed API.
//
// Two text-only fixtures are seeded via the API: one regular post and
// one album. The album row exists in the same `posts` table but with
// `kind = 'album'`. The feed handler hard-filters to `kind = 'post'`
// (we deliberately dropped the `?kinds=` override — the album surface
// has its own discovery path on the left rail and `/albums`).
//
// S3-free: both fixtures are text-only so this spec runs in the CI
// test env without an object store. Lightbox / media-engagement specs
// that exercise the full upload + serve path are deferred to the
// MinIO follow-up.

describe("feed kind discriminator", () => {
  it("posts appear in /api/feed; album-kind rows do not", () => {
    cy.login();

    let postId;
    let albumId;
    cy.createPost({ body: "feed-kinds spec post" }).then(({ body }) => {
      postId = body.id;
    });
    cy.createAlbum({
      name: "feed-kinds spec album",
      description: "should not appear in feed",
    }).then(({ body }) => {
      albumId = body.id;
    });

    cy.then(() => {
      cy.request("GET", "/api/feed").then(({ status, body }) => {
        expect(status).to.eq(200);
        const ids = body.posts.map((p) => p.id);
        expect(ids, "feed should include the seeded post").to.include(postId);
        expect(ids, "feed should not include the seeded album").to.not.include(
          albumId,
        );
      });
    });
  });

  it("albums appear in /api/albums; post-kind rows do not", () => {
    cy.login();

    let postId;
    let albumId;
    cy.createPost({ body: "feed-kinds spec post B" }).then(({ body }) => {
      postId = body.id;
    });
    cy.createAlbum({ name: "feed-kinds spec album B" }).then(({ body }) => {
      albumId = body.id;
    });

    cy.then(() => {
      cy.request("GET", "/api/albums").then(({ status, body }) => {
        expect(status).to.eq(200);
        const ids = body.albums.map((a) => a.id);
        expect(ids, "albums list should include the seeded album").to.include(
          albumId,
        );
        expect(
          ids,
          "albums list should not include post-kind rows",
        ).to.not.include(postId);
      });
    });
  });
});

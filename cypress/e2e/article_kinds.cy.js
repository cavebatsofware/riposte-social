// Feature spec: article kind discriminator on the feed + cross-kind
// 404s + drafts staying off the public surface.
//
// Mirrors feed_kinds.cy.js. Text-only (no cover image), matching the
// CI test env's S3-free constraint. Cover image upload + the inline
// markdown image flow are exercised by backend tests with the
// s3_mock helper.

describe("article kind discriminator", () => {
  it("published articles appear in /api/feed with kind=article and an embedded preview", () => {
    cy.login();

    let articleId;
    cy.createArticle({
      title: "Cypress published article",
      body: "Hello article body.",
    }).then(({ body }) => {
      articleId = body.id;
    });

    cy.then(() => {
      cy.request("GET", "/api/feed").then(({ status, body }) => {
        expect(status).to.eq(200);
        const match = body.posts.find((p) => p.id === articleId);
        expect(match, "article in feed").to.exist;
        expect(match.kind).to.eq("article");
        expect(match.article, "article preview embedded").to.exist;
        expect(match.article.title).to.eq("Cypress published article");
        expect(match.article.reading_time_minutes).to.be.at.least(1);
      });
    });
  });

  it("feed kind filter partitions posts and articles", () => {
    cy.login();

    let postId;
    let articleId;
    cy.createPost({ body: "kind-filter spec post" }).then(({ body }) => {
      postId = body.id;
    });
    cy.createArticle({ title: "kind-filter spec article" }).then(({ body }) => {
      articleId = body.id;
    });

    cy.then(() => {
      cy.request("GET", "/api/feed?kind=articles").then(({ body }) => {
        const ids = body.posts.map((p) => p.id);
        expect(ids).to.include(articleId);
        expect(ids).to.not.include(postId);
        body.posts.forEach((p) => expect(p.kind).to.eq("article"));
      });
      cy.request("GET", "/api/feed?kind=posts").then(({ body }) => {
        const ids = body.posts.map((p) => p.id);
        expect(ids).to.include(postId);
        expect(ids).to.not.include(articleId);
        body.posts.forEach((p) => expect(p.kind).to.eq("post"));
      });
    });
  });

  it("drafts stay out of the public listing", () => {
    cy.login();

    let draftId;
    cy.createArticle({
      title: "Cypress draft article",
      is_draft: true,
    }).then(({ body }) => {
      draftId = body.id;
    });

    cy.then(() => {
      cy.request("GET", "/api/articles").then(({ body }) => {
        const ids = body.articles.map((a) => a.id);
        expect(ids, "draft must not appear in /api/articles").to.not.include(
          draftId,
        );
      });
      cy.request("GET", "/api/feed").then(({ body }) => {
        const ids = body.posts.map((p) => p.id);
        expect(ids, "draft must not appear in /api/feed").to.not.include(
          draftId,
        );
      });
    });
  });

  it("cross-kind routing: post id fails on /api/articles and vice versa", () => {
    cy.login();

    let postId;
    let articleId;
    cy.createPost({ body: "cross-kind spec post" }).then(({ body }) => {
      postId = body.id;
    });
    cy.createArticle({ title: "cross-kind spec article" }).then(({ body }) => {
      articleId = body.id;
    });

    cy.then(() => {
      cy.request({
        url: `/api/posts/${articleId}`,
        failOnStatusCode: false,
      }).then(({ status }) => {
        expect(status).to.eq(404);
      });
      cy.request({
        url: `/api/articles/${postId}`,
        failOnStatusCode: false,
      }).then(({ status }) => {
        expect(status).to.eq(404);
      });
    });
  });

  it("drafts endpoint requires authentication and lists own drafts", () => {
    cy.request({
      url: "/api/articles/drafts",
      failOnStatusCode: false,
    }).then(({ status }) => {
      expect(status, "anonymous caller gets 401/403/404").to.be.oneOf([
        401, 403, 404,
      ]);
    });

    cy.login();
    let draftId;
    cy.createArticle({ title: "drafts endpoint spec", is_draft: true }).then(
      ({ body }) => {
        draftId = body.id;
      },
    );
    cy.then(() => {
      cy.request("GET", "/api/articles/drafts").then(({ status, body }) => {
        expect(status).to.eq(200);
        const ids = body.articles.map((a) => a.id);
        expect(ids).to.include(draftId);
      });
    });
  });
});

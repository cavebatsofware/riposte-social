// Feature spec: Open Graph meta on the `/post/{id}` and `/articles/{id}`
// permalink shells (src/og.rs).
//
// The endpoints always return the SPA shell with 200 so the client can
// still render the interactive page; what varies is whether per-content
// og:* / twitter:* meta is injected into <head>. Meta is emitted only for
// a row an anonymous visitor may see, so these assertions double as the
// no-leak guarantee: private posts, drafts, wrong-kind ids, missing ids,
// and the public-feed-off state must all yield the generic shell.
//
// The OG handler resolves visibility anonymously regardless of the
// caller's session, so an authed cy.request still gets the anonymous
// verdict; no logout dance is needed here. `public_feed_enabled` is off
// by default and gates injection, so the suite enables it first.
//
// S3-free: text-only fixtures (no cover / media), so og:image is absent
// and twitter:card falls back to "summary". The image path is exercised
// separately once the media seeding helper grows S3 support.

const OG_TITLE = 'property="og:title"';

describe("Open Graph share meta", () => {
  beforeEach(() => {
    cy.login();
    cy.setPublicFeed(true);
  });

  after(() => {
    // Restore the default so the flag does not bleed into later specs on
    // the persistent (non-reset) stack.
    cy.login();
    cy.setPublicFeed(false);
  });

  it("public post: injects og:title, og:url, description, and a twitter card", () => {
    cy.createPost({ body: "OG public post body sentence." }).then(({ body }) => {
      const id = body.id;
      cy.request(`/post/${id}`).then(({ status, body: html }) => {
        expect(status).to.eq(200);
        expect(html).to.include(OG_TITLE);
        expect(html).to.include('property="og:type"');
        expect(html).to.include('property="og:url"');
        expect(html).to.include(`/post/${id}`);
        expect(html).to.include('property="og:description"');
        expect(html, "description derives from the post body").to.include(
          "OG public post body",
        );
        expect(html).to.include('name="twitter:card"');
      });
    });
  });

  it("public article: og:title is the article title and og:url is the article path", () => {
    cy.createArticle({
      title: "OG Public Article Title",
      body: "Article OG body text.",
    }).then(({ body }) => {
      const id = body.id;
      cy.request(`/articles/${id}`).then(({ status, body: html }) => {
        expect(status).to.eq(200);
        expect(html).to.include(OG_TITLE);
        expect(html).to.include("OG Public Article Title");
        expect(html).to.include('property="og:url"');
        expect(html).to.include(`/articles/${id}`);
        expect(html).to.include('property="og:type"');
      });
    });
  });

  it("private post: serves the shell with no meta and no leaked body", () => {
    cy.createPost({
      body: "OG private secret body text.",
      visibility: "private",
    }).then(({ body }) => {
      const id = body.id;
      cy.request(`/post/${id}`).then(({ status, body: html }) => {
        expect(status).to.eq(200);
        expect(html).to.not.include(OG_TITLE);
        expect(html).to.not.include("OG private secret body");
        expect(html).to.not.include(`/post/${id}`);
      });
    });
  });

  it("draft article: serves the shell with no meta and no leaked title", () => {
    cy.createArticle({
      title: "OG Draft Article Secret",
      is_draft: true,
    }).then(({ body }) => {
      const id = body.id;
      cy.request(`/articles/${id}`).then(({ status, body: html }) => {
        expect(status).to.eq(200);
        expect(html).to.not.include(OG_TITLE);
        expect(html).to.not.include("OG Draft Article Secret");
      });
    });
  });

  it("kind guard: a post id at /articles/{id} injects no meta", () => {
    cy.createPost({ body: "cross-kind og post body." }).then(({ body }) => {
      cy.request(`/articles/${body.id}`).then(({ status, body: html }) => {
        expect(status).to.eq(200);
        expect(html).to.not.include(OG_TITLE);
      });
    });
  });

  it("kind guard: an article id at /post/{id} injects no meta", () => {
    cy.createArticle({ title: "Cross-kind OG article", body: "x" }).then(
      ({ body }) => {
        cy.request(`/post/${body.id}`).then(({ status, body: html }) => {
          expect(status).to.eq(200);
          expect(html).to.not.include(OG_TITLE);
        });
      },
    );
  });

  it("missing id: serves the shell with no meta", () => {
    cy.request(
      "/post/00000000-0000-0000-0000-000000000000",
    ).then(({ status, body: html }) => {
      expect(status).to.eq(200);
      expect(html).to.not.include(OG_TITLE);
    });
  });

  it("malformed id: serves the shell without crashing", () => {
    cy.request("/post/not-a-uuid").then(({ status, body: html }) => {
      expect(status).to.eq(200);
      expect(html).to.not.include(OG_TITLE);
    });
  });

  it("public-feed gate off: no meta even for a public post", () => {
    cy.createPost({ body: "gate toggle post body." }).then(({ body }) => {
      const id = body.id;
      cy.setPublicFeed(false);
      cy.request(`/post/${id}`).then(({ status, body: html }) => {
        expect(status).to.eq(200);
        expect(html).to.not.include(OG_TITLE);
        expect(html).to.not.include(`/post/${id}`);
      });
      // Restore so the flag's state doesn't bleed into later specs.
      cy.setPublicFeed(true);
    });
  });

  it("escapes HTML in the injected title", () => {
    cy.createArticle({
      title: 'Ampersand & "Quotes" Title',
      body: "escape body text.",
    }).then(({ body }) => {
      cy.request(`/articles/${body.id}`).then(({ body: html }) => {
        expect(html).to.include("Ampersand &amp; &quot;Quotes&quot; Title");
        expect(html, "raw unescaped title must not appear").to.not.include(
          'Ampersand & "Quotes" Title',
        );
      });
    });
  });
});

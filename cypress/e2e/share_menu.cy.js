// Feature spec: the visitor-facing ShareMenu on post and article
// permalinks (social-frontend/src/components/ShareMenu.tsx).
//
// Covers the three properties that define the feature:
//  - reachability gate: external targets appear only for public content;
//    copy-link is always offered to anyone who can see the item.
//  - anonymous reach: a signed-out visitor on a public post can share.
//  - copy behavior: clicking Copy link writes the canonical permalink to
//    the clipboard and surfaces the "Link copied" status.
//
// `public_feed_enabled` is enabled so the anonymous case can read the
// public post; it is off by default. Text-only fixtures (S3-free).

describe("ShareMenu", () => {
  beforeEach(() => {
    cy.login();
    cy.setPublicFeed(true);
  });

  it("public post: menu offers copy link plus external targets", () => {
    cy.createPost({ body: "share ui public post" }).then(({ body }) => {
      cy.visit(`/post/${body.id}`);
      cy.get(".share-picker-toggle", { timeout: 10000 })
        .should("be.visible")
        .click();
      cy.get(".share-picker-popover").should("be.visible");
      cy.contains(".share-picker-item", "Copy link").should("exist");
      cy.contains(".share-picker-item", "Facebook").should("exist");
      cy.contains(".share-picker-item", "Reddit").should("exist");
      cy.contains(".share-picker-item", "Email").should("exist");
      cy.contains(".share-picker-item", "Text message").should("exist");
      cy.contains(".share-picker-item", /^X$/).should("exist");
    });
  });

  it("reachability gate: a private post offers copy link but no external targets", () => {
    cy.createPost({
      body: "share ui private post",
      visibility: "private",
    }).then(({ body }) => {
      cy.visit(`/post/${body.id}`);
      cy.get(".share-picker-toggle", { timeout: 10000 }).click();
      cy.get(".share-picker-popover").should("be.visible");
      cy.contains(".share-picker-item", "Copy link").should("exist");
      cy.contains(".share-picker-item", "Facebook").should("not.exist");
      cy.contains(".share-picker-item", "Email").should("not.exist");
    });
  });

  it("copy link: writes the canonical permalink and shows the copied status", () => {
    cy.createPost({ body: "share ui copy post" }).then(({ body }) => {
      const id = body.id;
      cy.visit(`/post/${id}`, {
        onBeforeLoad(win) {
          if (!win.navigator.clipboard) {
            Object.defineProperty(win.navigator, "clipboard", {
              value: {},
              configurable: true,
            });
          }
          cy.stub(win.navigator.clipboard, "writeText")
            .resolves()
            .as("writeText");
        },
      });
      cy.get(".share-picker-toggle", { timeout: 10000 }).click();
      cy.contains(".share-picker-item", "Copy link").click();
      cy.get("@writeText").should("have.been.calledOnce");
      cy.get("@writeText")
        .its("firstCall.args.0")
        .should("include", `/post/${id}`);
      cy.get(".share-picker-status").should("contain", "Link copied");
    });
  });

  it("anonymous visitor can share a public post", () => {
    cy.createPost({ body: "share ui anon post" }).then(({ body }) => {
      const id = body.id;
      cy.clearCookies();
      cy.visit(`/post/${id}`);
      cy.get(".share-picker-toggle", { timeout: 10000 })
        .should("be.visible")
        .click();
      cy.contains(".share-picker-item", "Copy link").should("exist");
      cy.contains(".share-picker-item", "Facebook").should("exist");
    });
  });

  it("public article: menu offers external targets", () => {
    cy.createArticle({ title: "Share UI Article", body: "body" }).then(
      ({ body }) => {
        cy.visit(`/articles/${body.id}`);
        cy.get(".share-picker-toggle", { timeout: 10000 })
          .should("be.visible")
          .click();
        cy.contains(".share-picker-item", "Copy link").should("exist");
        cy.contains(".share-picker-item", "Telegram").should("exist");
      },
    );
  });

  it("draft article permalink has no share control", () => {
    cy.createArticle({ title: "Share UI Draft", is_draft: true }).then(
      ({ body }) => {
        cy.visit(`/articles/${body.id}`);
        cy.contains(".article-view-title", "Share UI Draft", {
          timeout: 10000,
        }).should("exist");
        cy.get(".share-picker-toggle").should("not.exist");
      },
    );
  });

  it("mastodon: inline instance input validates the host and builds the share URL", () => {
    cy.createPost({ body: "share ui mastodon post" }).then(({ body }) => {
      cy.visit(`/post/${body.id}`, {
        onBeforeLoad(win) {
          cy.stub(win, "open").as("open");
        },
      });
      cy.get(".share-picker-toggle", { timeout: 10000 }).click();
      cy.contains(".share-picker-item", "Mastodon").click();
      // The native prompt is gone; an in-app input appears instead.
      cy.get(".share-picker-instance-input").should("be.visible");

      // A userinfo-bearing value (the "good.example@evil.com" redirect
      // trick) is rejected with an inline error and does not navigate.
      cy.get(".share-picker-instance-input").type("evil.com@attacker.com");
      cy.contains(".share-picker-instance-actions .btn-primary", "Share").click();
      cy.get(".share-picker-instance-error").should("be.visible");
      cy.get("@open").should("not.have.been.called");

      // A valid instance opens that instance's /share endpoint.
      cy.get(".share-picker-instance-input").clear().type("mastodon.social");
      cy.contains(".share-picker-instance-actions .btn-primary", "Share").click();
      cy.get("@open").should(
        "have.been.calledWithMatch",
        /^https:\/\/mastodon\.social\/share\?text=/,
      );
    });
  });

  it("toggle exposes aria-expanded state and Escape closes the popover", () => {
    cy.createPost({ body: "share ui aria post" }).then(({ body }) => {
      cy.visit(`/post/${body.id}`);
      cy.get(".share-picker-toggle", { timeout: 10000 })
        .should("have.attr", "aria-expanded", "false")
        .click();
      cy.get(".share-picker-toggle").should(
        "have.attr",
        "aria-expanded",
        "true",
      );
      cy.get(".share-picker-popover").should("have.attr", "role", "dialog");
      cy.get("body").type("{esc}");
      cy.get(".share-picker-popover").should("not.exist");
    });
  });
});

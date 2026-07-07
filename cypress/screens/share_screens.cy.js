// Opt-in capture spec: writes screenshots of the ShareMenu in its key
// states for debugging and eyeballing the UI. It lives OUTSIDE
// cypress/e2e/ on purpose so the normal suite (cypress.config.js's
// specPattern, and CI's `bunx cypress run`) never picks it up. It is not
// an assertion spec; the real coverage is in cypress/e2e/share_menu.cy.js.
//
// Run it deliberately against the up test stack:
//   bun run test:e2e:screens
//
// Screenshots land in cypress/screenshots/share_screens.cy.js/ (gitignored).

describe("ShareMenu screenshots", () => {
  beforeEach(() => {
    cy.viewport(1280, 900);
    cy.login();
    cy.setPublicFeed(true);
  });

  it("public post: closed then open menu", () => {
    cy.createPost({
      body: "Sharing a public post so anyone with the link can pass it along.",
    }).then(({ body }) => {
      cy.visit(`/post/${body.id}`);
      cy.get(".share-picker-toggle", { timeout: 10000 }).should("be.visible");
      cy.screenshot("01-post-share-button-closed", { capture: "viewport" });
      cy.get(".share-picker-toggle").click();
      cy.get(".share-picker-popover").should("be.visible");
      cy.screenshot("02-post-share-menu-open", { capture: "viewport" });
    });
  });

  it("public post: link-copied status", () => {
    cy.createPost({ body: "Copying this link drops it straight on the clipboard." }).then(
      ({ body }) => {
        cy.visit(`/post/${body.id}`, {
          onBeforeLoad(win) {
            if (!win.navigator.clipboard) {
              Object.defineProperty(win.navigator, "clipboard", {
                value: {},
                configurable: true,
              });
            }
            cy.stub(win.navigator.clipboard, "writeText").resolves();
          },
        });
        cy.get(".share-picker-toggle", { timeout: 10000 }).click();
        cy.contains(".share-picker-item", "Copy link").click();
        cy.get(".share-picker-status").should("contain", "Link copied");
        cy.screenshot("03-post-link-copied", { capture: "viewport" });
      },
    );
  });

  it("private post: reachability gate (copy link only)", () => {
    cy.createPost({
      body: "A private post only the author can see, so external targets stay hidden.",
      visibility: "private",
    }).then(({ body }) => {
      cy.visit(`/post/${body.id}`);
      cy.get(".share-picker-toggle", { timeout: 10000 }).click();
      cy.get(".share-picker-popover").should("be.visible");
      cy.screenshot("04-private-post-copy-only", { capture: "viewport" });
    });
  });

  it("public article: open menu", () => {
    cy.createArticle({
      title: "Sharing Long-Form Articles",
      body: "Articles get the same visitor-facing share menu as posts.",
    }).then(({ body }) => {
      cy.visit(`/articles/${body.id}`);
      cy.get(".share-picker-toggle", { timeout: 10000 }).should("be.visible").click();
      cy.get(".share-picker-popover").should("be.visible");
      cy.screenshot("05-article-share-menu-open", { capture: "viewport" });
    });
  });

  it("article card in the listing: share button bottom-right", () => {
    cy.createArticle({
      title: "Feed Card Share Placement",
      body: "The feed article card keeps its Share button bottom-right, matching the post card.",
    }).then(() => {
      cy.visit("/articles");
      cy.get(".article-card .share-picker-toggle", { timeout: 10000 }).should(
        "be.visible",
      );
      // Element screenshot of a single card so the Share placement reads
      // clearly against the card's bounds.
      cy.get(".article-card").first().screenshot("06-article-card-share");
    });
  });
});

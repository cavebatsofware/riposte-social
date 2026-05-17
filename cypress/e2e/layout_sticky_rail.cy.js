// Spec: verify the left BrowseRail sticks below the sticky header as the
// feed scrolls and does not scroll off-screen.
//
// Requires a desktop-width viewport (>= 1025px); at narrower widths the
// grid collapses to a single column and sticky does not apply.
//
// Asserts:
// 1. On desktop, the rail has position: sticky in computed style.
// 2. On desktop, after scrolling the feed the rail's top edge sits ~24px
//    below the header's bottom edge (the preserved var(--spacing-6) gap).
// 3. On tablet (<= 1024px), the rail is not sticky.

describe("layout: sticky BrowseRail", () => {
  beforeEach(() => {
    cy.login();
    for (let i = 0; i < 10; i++) {
      cy.createPost({
        body: `# Sticky rail spec post ${i + 1}\n\nThis post has enough text to push the feed well past the viewport height so the sticky rail behaviour is reliably testable.`,
      });
    }
  });

  context("desktop viewport (1440x900)", () => {
    beforeEach(() => {
      cy.viewport(1440, 900);
      cy.visit("/");
      cy.get(".layout-rail-left", { timeout: 10000 }).should("exist");
    });

    it("rail has position: sticky", () => {
      cy.get(".layout-rail-left").then(($el) => {
        expect(window.getComputedStyle($el[0]).position).to.eq("sticky");
      });
    });

    it("rail top tracks header bottom after scrolling", () => {
      cy.scrollTo(0, 800);
      cy.get(".layout-header").then(($header) => {
        const headerBottom = $header[0].getBoundingClientRect().bottom;
        cy.get(".layout-rail-left").then(($rail) => {
          const railTop = $rail[0].getBoundingClientRect().top;
          // sticky top = header height + var(--spacing-6) gap (24px)
          expect(railTop, "rail should not overlap header").to.be.at.least(
            headerBottom,
          );
          expect(railTop, "rail should preserve grid gap below header").to.be.within(
            headerBottom + 20,
            headerBottom + 32,
          );
        });
      });
    });
  });

  context("tablet viewport (900x800)", () => {
    beforeEach(() => {
      cy.viewport(900, 800);
      cy.visit("/");
      cy.get("main#main-content", { timeout: 10000 }).should("exist");
    });

    it("rail is not sticky below the 1024px breakpoint", () => {
      cy.get(".layout-rail-left").then(($el) => {
        expect(window.getComputedStyle($el[0]).position).to.not.eq("sticky");
      });
    });
  });
});

/// Functional spec for the social-frontend Categories page and the
/// member-edit modal. Builds on cy.login() from the test infra.
/// Asserts:
///
/// 1. The page mounts under the seeded admin with an h1 and a "New
///    category" button (the create-affordance the page-level role
///    gate exposes to admins).
/// 2. After the create POST returns the success banner is rendered
///    with role="status" so screen readers announce it without
///    interrupting other speech, and after the delete the same
///    banner re-renders with the same role.
/// 3. Creating a `user_list` category and opening its Edit-members
///    modal produces an accessible dialog with role="dialog",
///    aria-modal="true", and aria-labelledby pointing at the visible
///    title. Escape closes the modal.
/// 4. The created category is cleaned up at the end of the run so
///    the seed admin's category set returns to its original shape.

describe("browse/categories (seeded test admin)", () => {
  beforeEach(() => {
    cy.login();
    cy.visit("/categories");
    cy.get("main#main-content", { timeout: 10000 }).should("exist");
    cy.get(".page-header h1", { timeout: 10000 }).should("exist");
  });

  it("renders an h1 and the New-category affordance for admins", () => {
    cy.get(".page-header h1").should("exist");
    cy.contains("button", /new category|nueva categor|nouvelle cat|新建类别|Neue Kategorie/i)
      .should("exist");
  });

  it("MemberModal is an accessible dialog and Escape closes it", () => {
    const testName = `e2e-aria-${Date.now()}`;

    // Open the create form, fill it out, choose user_list visibility,
    // submit. Auto-add of creator means the row is immediately
    // manageable + user_list.
    cy.contains("button", /new category|nueva categor|nouvelle cat|新建类别|Neue Kategorie/i)
      .click();
    cy.get("#cat-new-name").type(testName, { delay: 0 });
    cy.get("#cat-new-vis").select("user_list");
    cy.get(".categories-form-actions button[type='submit']")
      .scrollIntoView()
      .click();

    // The create POST returns; the page-level success banner renders
    // with role="status" so screen readers announce it without
    // interrupting other speech.
    cy.get(".alert.alert-success", { timeout: 10000 })
      .should("have.attr", "role", "status");

    // Find the row for our new category and open the member modal. The
    // manageable row renders the name inside `<input value="...">` rather
    // than as text content, so target via the input's value attribute and
    // walk up to the row container.
    cy.get(`.categories-manage-row input.categories-input-name[value="${testName}"]`, {
      timeout: 10000,
    })
      .parents(".categories-manage-row")
      .as("testRow");
    cy.get("@testRow")
      .find("button")
      .contains(/edit members|editar miembros|modifier les membres|编辑成员|Mitglieder bearbeiten/i)
      .click();

    // Dialog semantics.
    cy.get(".categories-member-modal", { timeout: 10000 })
      .should("have.attr", "role", "dialog")
      .and("have.attr", "aria-modal", "true")
      .and("have.attr", "aria-labelledby", "member-modal-title");
    cy.get("#member-modal-title").should("exist");
    // The member-list region is grouped under the modal title for
    // assistive tech, and the search input is reachable via its sr-only
    // label (htmlFor=member-modal-search).
    cy.get(".categories-member-list").should(
      "have.attr",
      "aria-labelledby",
      "member-modal-title",
    );
    cy.get('label[for="member-modal-search"]').should("exist");

    // Press Escape inside the modal. The trap routes Escape through
    // onClose, so the modal unmounts.
    cy.get("#member-modal-search").type("{esc}");
    cy.get(".categories-member-modal").should("not.exist");

    // Cleanup. The row is clean (no draft change), so Delete is the
    // visible destructive action. Cypress auto-accepts window.confirm.
    cy.get("@testRow")
      .find("button.btn-danger")
      .scrollIntoView()
      .click();
    cy.get(`.categories-manage-row input.categories-input-name[value="${testName}"]`)
      .should("not.exist");
    // Delete also surfaces the same role="status" success banner.
    cy.get(".alert.alert-success", { timeout: 10000 })
      .should("have.attr", "role", "status");
  });
});

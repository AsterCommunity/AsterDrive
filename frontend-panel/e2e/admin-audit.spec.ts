import { createTeamViaApi } from "./support/api";
import { authenticate, gotoAdminPage } from "./support/auth";
import { uniqueName } from "./support/fixtures";
import { expect, test } from "./support/test";

test.describe
	.serial("Admin Audit E2E", () => {
		test("filters audit logs by action and entity type", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const teamName = uniqueName("pw-audit-team");
			await createTeamViaApi(
				page,
				teamName,
				"Team created to verify admin audit filters",
			);

			await gotoAdminPage(
				page,
				"/admin/audit?action=team_create&entityType=team",
				"Audit Log",
			);

			await expect(page.getByPlaceholder("Filter by action...")).toHaveValue(
				"team_create",
			);
			await expect(page.getByText("Filters active")).toBeVisible();

			const auditRow = page
				.getByRole("row")
				.filter({ hasText: teamName })
				.first();
			await expect(auditRow).toBeVisible({ timeout: 30_000 });
			await expect(auditRow).toContainText("Created team");
			await expect(auditRow).toContainText("Team");

			await page.getByRole("button", { name: "Clear filters" }).click();
			await expect(page.getByPlaceholder("Filter by action...")).toHaveValue(
				"",
			);
			await expect(page).toHaveURL(/\/admin\/audit$/);
		});
	});

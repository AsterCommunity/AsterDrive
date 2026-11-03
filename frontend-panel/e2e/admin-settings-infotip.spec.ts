import type { APIRequestContext, Page } from "@playwright/test";
import { authenticate, resolveAdminSiteUrlPrompt } from "./support/auth";
import { expect, test } from "./support/test";

const PUBLIC_SITE_URL_DESCRIPTION =
	"Set the public HTTP(S) origins that external users actually use to reach this site, one origin per field, for example https://drive.example.com and https://panel.example.com. They are used to build absolute links for share pages, Office previews, WebDAV, and callbacks; the first origin is the fallback. Leave the list empty to fall back to the current request origin.";

async function openSiteSettings(page: Page, request: APIRequestContext) {
	await authenticate(page, request);
	await page.goto("/admin/settings/site");
	await resolveAdminSiteUrlPrompt(page);

	const trigger = page.getByRole("button", {
		name: "Show description for Public site URL",
	});
	await expect(trigger).toBeVisible();
	return trigger;
}

test.describe("Admin settings description infotips", () => {
	test("keeps desktop hover access to the complete description", async ({
		page,
		request,
	}) => {
		const trigger = await openSiteSettings(page, request);

		await trigger.hover();

		await expect(page.getByText(PUBLIC_SITE_URL_DESCRIPTION)).toBeVisible();
		await expect(trigger).toHaveAttribute("aria-expanded", "true");
	});
});

test.describe("Admin settings description infotips on touch screens", () => {
	test.use({
		hasTouch: true,
		viewport: { width: 390, height: 844 },
	});

	test("opens and closes the complete description by tapping", async ({
		page,
		request,
	}) => {
		const trigger = await openSiteSettings(page, request);
		const description = page.getByText(PUBLIC_SITE_URL_DESCRIPTION);

		await trigger.tap();
		await expect(description).toBeVisible();
		await expect(trigger).toHaveAttribute("aria-expanded", "true");

		await trigger.tap();
		await expect(description).toBeHidden();
		await expect(trigger).toHaveAttribute("aria-expanded", "false");

		await trigger.tap();
		await expect(description).toBeVisible();
		await page.getByRole("heading", { name: "Settings", level: 2 }).tap();
		await expect(description).toBeHidden();
	});
});

import type { Locator, Page } from "@playwright/test";
import { createTeamViaApi } from "./support/api";
import { authenticate, logout } from "./support/auth";
import { uniqueName } from "./support/fixtures";
import { expect, test } from "./support/test";
import { disablePasskeyBrowserSupport } from "./support/webauthn";

async function assertClippedControl(page: Page, source: Locator) {
	await source.evaluate((element) => {
		const wrapper = document.createElement("div");
		wrapper.id = "focus-clip-fixture";
		wrapper.style.cssText =
			"position:fixed;top:40px;left:40px;width:240px;overflow:hidden;z-index:9999;background:white";
		const control = element.cloneNode(false) as
			| HTMLInputElement
			| HTMLTextAreaElement;
		control.removeAttribute("id");
		control.removeAttribute("aria-invalid");
		control.style.cssText =
			"display:block;width:100%;height:48px;min-height:48px;transition:none;background:white!important;border-color:gray!important";
		wrapper.append(control);
		document.body.append(wrapper);
	});
	const control = page.locator("#focus-clip-fixture > :first-child");
	await control.blur();
	const rect = await control.boundingBox();
	const wrapper = await page.locator("#focus-clip-fixture").boundingBox();
	if (!rect || !wrapper) throw new Error("Focus fixture is not visible");
	expect(rect.x).toBe(wrapper.x);
	expect(rect.width).toBe(wrapper.width);
	const left = { x: rect.x + 2, y: rect.y + 15, width: 2, height: 12 };
	const right = { ...left, x: rect.x + rect.width - 4 };
	const beforeLeft = await page.screenshot({ clip: left });
	const beforeRight = await page.screenshot({ clip: right });
	await control.focus();
	await expect(control).toHaveJSProperty("disabled", false);
	await expect
		.poll(() => control.evaluate((el) => el.matches(":focus-visible")))
		.toBe(true);
	await expect
		.poll(() => control.evaluate((el) => getComputedStyle(el).boxShadow))
		.toContain("inset");
	expect(await page.screenshot({ clip: left })).not.toEqual(beforeLeft);
	expect(await page.screenshot({ clip: right })).not.toEqual(beforeRight);
	await control.blur();
	await control.evaluate((el) => el.setAttribute("aria-invalid", "true"));
	expect(await page.screenshot({ clip: left })).not.toEqual(beforeLeft);
	expect(await page.screenshot({ clip: right })).not.toEqual(beforeRight);
	await expect
		.poll(() => control.evaluate((el) => getComputedStyle(el).boxShadow))
		.toContain("inset");
	const invalidShadow = await control.evaluate(
		(el) => getComputedStyle(el).boxShadow,
	);
	await control.focus();
	await expect
		.poll(() => control.evaluate((el) => getComputedStyle(el).boxShadow))
		.toBe(invalidShadow);
	await page.emulateMedia({ forcedColors: "active" });
	await expect
		.poll(() => control.evaluate((el) => getComputedStyle(el).outlineStyle))
		.not.toBe("none");
	await page.emulateMedia({ forcedColors: "none" });
	await page.locator("#focus-clip-fixture").evaluate((el) => el.remove());
}

test("text controls keep inset focus and invalid indicators at clipped edges", async ({
	page,
	request,
}, testInfo) => {
	await disablePasskeyBrowserSupport(page);
	await authenticate(page, request);
	const team = await createTeamViaApi(page, uniqueName("focus-control"));
	await page.goto(`/admin/teams/${team.id}/overview`);
	await expect(page.locator("#admin-team-detail-description")).toBeVisible();
	await assertClippedControl(
		page,
		page.locator("#admin-team-detail-description"),
	);
	await logout(page);
	await page.goto("/login");
	await expect(page.locator("#identifier")).toBeVisible();
	await assertClippedControl(page, page.locator("#identifier"));
	for (const [name, width, height] of [
		["desktop", 1440, 960],
		["mobile", 390, 844],
	] as const) {
		await page.setViewportSize({ width, height });
		const capture = async (panel: string, selector: string) => {
			const field = page.locator(selector);
			await expect(field).toBeVisible();
			await expect(
				field.locator("xpath=ancestor::*[@aria-hidden='true']"),
			).toHaveCount(0);
			await expect
				.poll(() =>
					field.evaluate((element) => {
						for (
							let parent: Element | null = element;
							parent;
							parent = parent.parentElement
						) {
							if (Number(getComputedStyle(parent).opacity) < 0.99) return false;
						}
						return true;
					}),
				)
				.toBe(true);
			await field.focus();
			await expect
				.poll(() => field.evaluate((el) => getComputedStyle(el).boxShadow))
				.toContain("inset");
			await testInfo.attach(`${panel}-focus-${name}`, {
				body: await page.screenshot({ animations: "disabled" }),
				contentType: "image/png",
			});
		};
		await page.goto("/login");
		await capture("login", "#identifier");
		await page.getByRole("button", { name: "Sign Up", exact: true }).click();
		await capture("register", "#extra");
		await page.goto("/login");
		await page.getByRole("button", { name: "Forgot password?" }).click();
		await capture("password-reset", "#password-reset-email");
		await page.goto("/login?external_auth=email_required&flow=visual-test");
		await capture(
			"external-recovery",
			"#external-auth-password-link-identifier",
		);
	}
});

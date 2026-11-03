import type { Locator, Page } from "@playwright/test";
import { createTeamViaApi } from "./support/api";
import { authenticate, logout } from "./support/auth";
import { expectCodePreview, uploadViaPicker } from "./support/files";
import { CODE_FILE, uniqueName } from "./support/fixtures";
import { expect, test } from "./support/test";
import { disablePasskeyBrowserSupport } from "./support/webauthn";

async function assertUnchangedLayout(page: Page) {
	const { actual, original } = await page.evaluate(() => {
		const geometry = () =>
			Array.from(document.querySelectorAll("input, textarea, label, button"))
				.filter((element) => element.getClientRects().length > 0)
				.map((element) => {
					const { x, y, width, height } = element.getBoundingClientRect();
					return { x, y, width, height };
				});
		const actual = geometry();
		const reset = document.createElement("style");
		reset.textContent =
			".focus-ring-space { margin: 0 !important; padding: 0 !important; }";
		document.head.append(reset);
		const original = geometry();
		reset.remove();
		return { actual, original };
	});
	expect(actual).toEqual(original);
}

async function assertRingFitsAncestors(field: Locator) {
	const clipped = await field.evaluate((element) => {
		const rect = element.getBoundingClientRect();
		const failures: string[] = [];
		for (
			let parent = element.parentElement;
			parent;
			parent = parent.parentElement
		) {
			const style = getComputedStyle(parent);
			const bounds = parent.getBoundingClientRect();
			if (
				/(hidden|clip|auto|scroll)/.test(style.overflowX) &&
				(rect.left - 3 < bounds.left - 0.5 ||
					rect.right + 3 > bounds.right + 0.5)
			) {
				failures.push(`${parent.tagName}.${parent.className}: horizontal`);
			}
			if (
				/(hidden|clip|auto|scroll)/.test(style.overflowY) &&
				(rect.top - 3 < bounds.top - 0.5 ||
					rect.bottom + 3 > bounds.bottom + 0.5)
			) {
				failures.push(`${parent.tagName}.${parent.className}: vertical`);
			}
		}
		return failures;
	});
	expect(clipped).toEqual([]);
}

async function assertClippedControl(page: Page, source: Locator) {
	await source.evaluate((element) => {
		const host = document.createElement("div");
		host.id = "focus-clip-host";
		host.style.cssText =
			"position:fixed;top:40px;left:40px;width:240px;z-index:9999;background:white";
		const wrapper = document.createElement("div");
		wrapper.className = "focus-ring-space";
		wrapper.id = "focus-clip-fixture";
		wrapper.style.cssText = "overflow:hidden;background:white";
		const control = element.cloneNode(false) as
			| HTMLInputElement
			| HTMLTextAreaElement;
		control.removeAttribute("id");
		control.removeAttribute("aria-invalid");
		control.style.cssText =
			"display:block;height:48px;min-height:48px;transition:none;background:white!important;border-color:gray!important";
		wrapper.append(control);
		host.append(wrapper);
		document.body.append(host);
	});
	const control = page.locator("#focus-clip-fixture > :first-child");
	await control.blur();
	const rect = await control.boundingBox();
	const wrapper = await page.locator("#focus-clip-fixture").boundingBox();
	if (!rect || !wrapper) throw new Error("Focus fixture is not visible");
	expect(rect.x - wrapper.x).toBe(3);
	expect(wrapper.x + wrapper.width - rect.x - rect.width).toBe(3);
	expect(rect.y - wrapper.y).toBe(3);
	expect(wrapper.y + wrapper.height - rect.y - rect.height).toBe(3);
	const left = { x: rect.x - 3, y: rect.y + 15, width: 2, height: 12 };
	const right = { ...left, x: rect.x + rect.width + 1 };
	const beforeLeft = await page.screenshot({ clip: left });
	const beforeRight = await page.screenshot({ clip: right });
	await control.focus();
	await expect(control).toHaveJSProperty("disabled", false);
	await expect
		.poll(() => control.evaluate((el) => el.matches(":focus-visible")))
		.toBe(true);
	await expect
		.poll(() => control.evaluate((el) => getComputedStyle(el).boxShadow))
		.toMatch(/0px 0px 0px 3px(?:,|$)/);
	expect(await page.screenshot({ clip: left })).not.toEqual(beforeLeft);
	expect(await page.screenshot({ clip: right })).not.toEqual(beforeRight);
	expect(await control.boundingBox()).toEqual(rect);
	await control.blur();
	await control.evaluate((el) => el.setAttribute("aria-invalid", "true"));
	expect(await page.screenshot({ clip: left })).not.toEqual(beforeLeft);
	expect(await page.screenshot({ clip: right })).not.toEqual(beforeRight);
	await expect
		.poll(() => control.evaluate((el) => getComputedStyle(el).boxShadow))
		.toMatch(/0px 0px 0px 3px(?:,|$)/);
	const invalidShadow = await control.evaluate(
		(el) => getComputedStyle(el).boxShadow,
	);
	await control.focus();
	await expect
		.poll(() => control.evaluate((el) => getComputedStyle(el).boxShadow))
		.not.toBe(invalidShadow);
	expect(await control.boundingBox()).toEqual(rect);
	await page.emulateMedia({ forcedColors: "active" });
	await expect
		.poll(() => control.evaluate((el) => getComputedStyle(el).outlineStyle))
		.toBe("solid");
	await expect(control).toHaveCSS("outline-width", "2px");
	await expect(control).toHaveCSS("outline-offset", "0px");
	await page.emulateMedia({ forcedColors: "none" });
	await page.locator("#focus-clip-host").evaluate((el) => el.remove());
}

test("clipping wrappers preserve outer focus rings without changing control layout", async ({
	page,
	request,
}, testInfo) => {
	await disablePasskeyBrowserSupport(page);
	await authenticate(page, request);
	const team = await createTeamViaApi(page, uniqueName("focus-control"));
	await page.goto(`/admin/teams/${team.id}/overview`);
	await expect(page.locator("#admin-team-detail-description")).toBeVisible();
	await assertUnchangedLayout(page);
	await assertClippedControl(
		page,
		page.locator("#admin-team-detail-description"),
	);
	await page.evaluate(() => document.documentElement.classList.add("dark"));
	await assertClippedControl(
		page,
		page.locator("#admin-team-detail-description"),
	);
	await page.evaluate(() => document.documentElement.classList.remove("dark"));
	await page.goto("/admin/policies/new");
	await page.getByRole("button", { name: "Local", exact: true }).click();
	await page.getByRole("button", { name: "Next", exact: true }).click();
	await expect(page.locator("#name")).toBeVisible();
	await expect
		.poll(() =>
			page
				.getByTestId("policy-step-panel")
				.evaluate((element) =>
					element
						.getAnimations()
						.every((animation) => animation.playState === "finished"),
				),
		)
		.toBe(true);
	await assertUnchangedLayout(page);
	await assertRingFitsAncestors(page.locator("#name"));
	await assertRingFitsAncestors(page.locator("#base_path"));
	await logout(page);
	await page.goto("/login");
	await expect(page.locator("#identifier")).toBeVisible();
	await assertClippedControl(page, page.locator("#identifier"));
	await page.evaluate(() => document.documentElement.classList.add("dark"));
	await assertClippedControl(page, page.locator("#identifier"));
	await page.evaluate(() => document.documentElement.classList.remove("dark"));
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
			await assertUnchangedLayout(page);
			for (const input of await page.locator("input:visible").all()) {
				await assertRingFitsAncestors(input);
			}
			await expect
				.poll(() => field.evaluate((el) => getComputedStyle(el).boxShadow))
				.toMatch(/0px 0px 0px 3px(?:,|$)/);
			await testInfo.attach(`${panel}-focus-${name}`, {
				body: await page.screenshot({ animations: "disabled" }),
				contentType: "image/png",
			});
		};
		await page.goto("/login");
		await capture("login", "#identifier");
		await page.getByRole("button", { name: "Sign Up", exact: true }).click();
		await capture("register", "#extra");
		const collapsedHeight = await page.locator("#extra").evaluate((element) => {
			const grid = element.closest(".focus-ring-space")?.parentElement;
			if (!grid) throw new Error("Animated grid not found");
			const original = grid.style.cssText;
			grid.style.transition = "none";
			grid.style.gridTemplateRows = "0fr";
			const height = grid.getBoundingClientRect().height;
			grid.style.cssText = original;
			return height;
		});
		expect(collapsedHeight).toBe(0);
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

test("borderless code editor keeps its overlay aligned and exposes keyboard focus", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	await uploadViaPicker(page, [CODE_FILE]);
	await expectCodePreview(page, CODE_FILE.name);
	await page.getByRole("button", { name: "Edit", exact: true }).click();
	const editor = page.locator(".code-preview-editor-input");
	await editor.focus();
	await expect(editor).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
	await expect(editor).toHaveCSS("outline-style", "solid");
	await expect(editor).toHaveCSS("outline-width", "2px");
	await expect(editor).toHaveCSS("outline-offset", "-2px");
	const aligned = await editor.evaluate((element) => {
		const overlay = element.parentElement?.querySelector("pre");
		if (!overlay) throw new Error("Highlight overlay not found");
		const inputRect = element.getBoundingClientRect();
		const overlayRect = overlay.getBoundingClientRect();
		const inputStyle = getComputedStyle(element);
		const overlayStyle = getComputedStyle(overlay);
		return (
			inputRect.x === overlayRect.x &&
			inputRect.y === overlayRect.y &&
			inputStyle.padding === overlayStyle.padding &&
			inputStyle.fontSize === overlayStyle.fontSize &&
			inputStyle.lineHeight === overlayStyle.lineHeight
		);
	});
	expect(aligned).toBe(true);
});

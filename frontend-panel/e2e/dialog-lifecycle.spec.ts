import type { Locator } from "@playwright/test";
import { authenticate } from "./support/auth";
import {
	fileNameCell,
	openItemContextMenu,
	uploadViaPicker,
} from "./support/files";
import { IMAGE_FILE, uniqueName } from "./support/fixtures";
import { expect, test } from "./support/test";

async function observeExit(dialog: Locator, selector: string) {
	await dialog.evaluate((node, selector) => {
		const observer = new MutationObserver(() => {
			if (!node.hasAttribute("data-closed")) return;
			const content = node.querySelector<HTMLInputElement | HTMLImageElement>(
				selector,
			);
			node.setAttribute(
				"data-exit-content",
				content instanceof HTMLInputElement
					? content.value
					: (content?.getAttribute("src") ?? "missing"),
			);
			for (const animation of node.getAnimations({ subtree: true })) {
				if (animation.effect?.getComputedTiming().iterations !== Infinity)
					animation.pause();
			}
			observer.disconnect();
		});
		observer.observe(node, {
			attributes: true,
			attributeFilter: ["data-closed"],
		});
	}, selector);
}

async function finishExit(dialog: Locator) {
	await dialog.evaluate((node) => {
		for (const animation of node.getAnimations({ subtree: true })) {
			if (animation.playState === "paused") animation.finish();
		}
	});
	await expect(dialog).toHaveCount(0);
}

test("rapid reopen ignores the old exit and supports animation cancellation", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	const trigger = page.getByRole("button", { name: "Open search" }).first();
	await trigger.click();
	const dialog = page.locator('[data-slot="dialog-content"]');
	await dialog
		.getByPlaceholder("Search files and folders...")
		.fill("old session");
	await observeExit(dialog, "input");
	await page.keyboard.press("Escape");
	await expect(dialog).toHaveAttribute("data-exit-content", "old session");
	// Invoke the real trigger while the former popup is still animating out.
	await trigger.evaluate((node) => node.click());
	await expect(
		dialog.getByPlaceholder("Search files and folders..."),
	).toHaveValue("");
	await expect(dialog).not.toHaveAttribute("data-closed");
	await observeExit(dialog, "input");
	await page.keyboard.press("Escape");
	await expect(dialog).toHaveAttribute("data-closed");
	await dialog.evaluate((node) => {
		for (const animation of node.getAnimations({ subtree: true }))
			animation.cancel();
	});
	await expect(dialog).toHaveCount(0);
});

test("no-animation backdrop dismissal still disposes content", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	await page.addStyleTag({
		content:
			'[data-slot="dialog-content"], [data-slot="dialog-overlay"] { animation: none !important; transition: none !important; }',
	});
	await page.getByRole("button", { name: "Open search" }).first().click();
	await expect(
		page.getByRole("dialog", { name: "Search", exact: true }),
	).toBeVisible();
	await page
		.locator('[data-slot="dialog-overlay"]')
		.click({ position: { x: 4, y: 4 } });
	await expect(page.locator('[data-slot="dialog-content"]')).toHaveCount(0);
});

test("search keeps the closing query and restores focus after actual exit", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	const trigger = page.getByRole("button", { name: "Open search" }).first();
	await trigger.click();
	const dialog = page.locator('[data-slot="dialog-content"]');
	await dialog
		.getByPlaceholder("Search files and folders...")
		.fill("retained search");
	await observeExit(dialog, "input");
	await page.keyboard.press("Escape");
	await expect(dialog).toHaveAttribute("data-exit-content", "retained search");
	await finishExit(dialog);
	await expect(trigger).toBeFocused();
	await trigger.click();
	await expect(
		dialog.getByPlaceholder("Search files and folders..."),
	).toHaveValue("");
});

test("avatar crop retains its object URL while exiting and releases it afterwards", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	await page.goto("/settings/profile");
	await page.evaluate(() => {
		const original = URL.revokeObjectURL.bind(URL);
		URL.revokeObjectURL = (url) => {
			document.documentElement.setAttribute("data-last-revoked-url", url);
			original(url);
		};
	});
	await page.locator('input[type="file"]').setInputFiles(IMAGE_FILE);
	const dialog = page.locator('[data-slot="dialog-content"]');
	await expect(
		dialog.getByRole("heading", { name: "Crop avatar" }),
	).toBeVisible();
	const source = await dialog.locator("img").getAttribute("src");
	expect(source).toMatch(/^blob:/);
	await observeExit(dialog, "img");
	await page.keyboard.press("Escape");
	await expect(dialog).toHaveAttribute("data-exit-content", source as string);
	await expect(page.locator("html")).not.toHaveAttribute(
		"data-last-revoked-url",
		source as string,
	);
	await finishExit(dialog);
	await expect(page.locator("html")).toHaveAttribute(
		"data-last-revoked-url",
		source as string,
	);
});

test("nested tag dialogs restore focus to their parent and then the file browser", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	const name = `${uniqueName("dialog-tags")}.txt`;
	await uploadViaPicker(page, [
		{ name, mimeType: "text/plain", buffer: Buffer.from("tags") },
	]);
	await expect(fileNameCell(page, name)).toBeVisible();
	await openItemContextMenu(page, name);
	await page
		.getByRole("menuitem", { name: "Manage tags", exact: true })
		.click();
	const parent = page.getByRole("dialog", { name: "Manage tags" });
	const trigger = parent.getByRole("button", { name: "Manage library" });
	await trigger.click();
	const nested = page.getByRole("dialog", { name: "Manage tag library" });
	await expect(nested).toBeVisible();
	await page.keyboard.press("Escape");
	await expect(nested).toHaveCount(0);
	await expect(parent).toBeVisible();
	await expect(trigger).toBeFocused();
	await page.keyboard.press("Escape");
	await expect(parent).toHaveCount(0);
	await expect(fileNameCell(page, name)).toBeVisible();
});

for (const mobile of [false, true]) {
	test(`reduced motion and reopen dispose search correctly (${mobile ? "mobile" : "desktop"})`, async ({
		page,
		request,
	}) => {
		if (mobile) await page.setViewportSize({ width: 390, height: 844 });
		await page.emulateMedia({ reducedMotion: "reduce" });
		await authenticate(page, request);
		// Keyboard shortcut is also available when the mobile sidebar is collapsed.
		await page.keyboard.press("ControlOrMeta+k");
		const dialog = page.getByRole("dialog", { name: "Search", exact: true });
		await expect(dialog).toBeVisible();
		await dialog.getByPlaceholder("Search files and folders...").fill("old");
		await page.keyboard.press("Escape");
		await expect(dialog).toHaveCount(0);
		await page.keyboard.press("ControlOrMeta+k");
		await expect(dialog).toBeVisible();
		await expect(
			dialog.getByPlaceholder("Search files and folders..."),
		).toHaveValue("");
		await page.keyboard.press("Escape");
		await expect(dialog).toHaveCount(0);
	});
}

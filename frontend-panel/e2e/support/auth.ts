import { type APIRequestContext, expect, type Page } from "@playwright/test";
import { fileDropZone } from "./files";
import {
	ADMIN,
	DEFAULT_STORAGE_STATE,
	PREVIEW_APPS_CACHE_KEY,
} from "./fixtures";

export async function seedClientState(
	page: Page,
	entries: Record<string, string> = { ...DEFAULT_STORAGE_STATE },
) {
	await page.addInitScript((storageEntries) => {
		for (const [key, value] of Object.entries(storageEntries)) {
			window.localStorage.setItem(key, value);
		}
	}, entries);
}

export async function captureClientState(page: Page) {
	const entries: Record<string, string> = { ...DEFAULT_STORAGE_STATE };
	const cachedPreviewApps = await page.evaluate(
		(key) => window.localStorage.getItem(key),
		PREVIEW_APPS_CACHE_KEY,
	);

	if (cachedPreviewApps) {
		entries[PREVIEW_APPS_CACHE_KEY] = cachedPreviewApps;
	}

	return entries;
}

export async function resolveAdminSiteUrlPrompt(page: Page) {
	const dialog = page.getByRole("alertdialog", {
		name: "Current site URL does not match the system config",
	});
	const promptAppeared = await dialog
		.waitFor({ state: "visible", timeout: 3_000 })
		.then(() => true)
		.catch(() => false);
	if (!promptAppeared) {
		return;
	}
	await dialog.getByRole("button", { name: "Update site URL" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
}

export async function gotoAdminPage(page: Page, url: string, heading: string) {
	await page.goto(url);
	await resolveAdminSiteUrlPrompt(page);
	await expect(page.getByRole("heading", { name: heading })).toBeVisible({
		timeout: 30_000,
	});
}

export async function hasUsers(request: APIRequestContext) {
	const response = await request.post("/api/v1/auth/check");
	expect(response.ok()).toBe(true);
	const payload = (await response.json()) as {
		code: number;
		data?: {
			has_users?: boolean;
		} | null;
	};
	expect(payload.code).toBe(0);
	return payload.data?.has_users ?? false;
}

export async function authenticate(page: Page, request: APIRequestContext) {
	if (await hasUsers(request)) {
		await loginAsAdmin(page);
		return;
	}

	await setupAdmin(page);
}

export async function setupAdmin(page: Page) {
	await page.goto("/login");
	await expect(page.locator("#extra")).toBeVisible();
	await page.locator("#identifier").fill(ADMIN.email);
	await page.locator("#extra").fill(ADMIN.username);
	await page.locator("#password").fill(ADMIN.password);
	await page.locator("form button[type='submit']").click();
	await expect(page).toHaveURL(/\/$/);
	await expect(fileDropZone(page)).toBeVisible();
	await ensureCurrentPublicSiteUrl(page);
}

export async function loginAsAdmin(page: Page) {
	await loginWithCredentials(page, ADMIN.email, ADMIN.password);
	await ensureCurrentPublicSiteUrl(page);
}

export async function ensureCurrentPublicSiteUrl(page: Page) {
	await page.evaluate(async () => {
		const readCookie = (name: string) => {
			const encodedName = `${encodeURIComponent(name)}=`;
			for (const chunk of document.cookie.split(";")) {
				const trimmed = chunk.trim();
				if (trimmed.startsWith(encodedName)) {
					return decodeURIComponent(trimmed.slice(encodedName.length));
				}
			}
			return null;
		};
		const csrfToken = readCookie("aster_csrf");
		const headers: Record<string, string> = {
			"Content-Type": "application/json",
		};
		if (csrfToken) {
			headers["X-CSRF-Token"] = csrfToken;
		}

		const response = await fetch("/api/v1/admin/config/public_site_url", {
			body: JSON.stringify({ value: [window.location.origin] }),
			credentials: "include",
			headers,
			method: "PUT",
		});
		if (!response.ok) {
			throw new Error(
				`failed to configure public_site_url: ${response.status} ${await response.text()}`,
			);
		}
	});
}

export async function loginWithCredentials(
	page: Page,
	identifier: string,
	password: string,
) {
	await page.goto("/login");
	await page.locator("#identifier").fill(identifier);
	await page.locator("#password").fill(password);
	await page.locator("form button[type='submit']").click();
	await expect(page).toHaveURL(/\/$/);
	await expect(fileDropZone(page)).toBeVisible();
}

export async function openUserMenu(page: Page, userName = ADMIN.username) {
	await page.getByRole("button", { name: userName }).click();
}

export async function logout(page: Page, userName = ADMIN.username) {
	await openUserMenu(page, userName);
	await page.getByRole("button", { name: "Logout" }).click();
	await expect(page).toHaveURL(/\/login$/);
}

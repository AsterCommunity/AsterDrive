import { hasUsers, loginAsAdmin, logout, setupAdmin } from "./support/auth";
import { expect, test } from "./support/test";
import {
	capturePasskeyGetCalls,
	disablePasskeyBrowserSupport,
	installPasskeyBrowserMock,
	mockPasskeyLoginEndpoints,
	readPasskeyGetCalls,
	resolvePendingPasskeyGet,
} from "./support/webauthn";

test.describe
	.serial("Auth E2E", () => {
		test("creates the initial admin, logs out, and signs back in", async ({
			page,
			request,
		}) => {
			await disablePasskeyBrowserSupport(page);
			expect(await hasUsers(request)).toBe(false);
			await setupAdmin(page);
			expect(await hasUsers(request)).toBe(true);

			await logout(page);
			await loginAsAdmin(page);
		});

		test("preserves caret position when editing login inputs in the middle", async ({
			page,
			request,
		}) => {
			await disablePasskeyBrowserSupport(page);
			if (!(await hasUsers(request))) {
				await setupAdmin(page);
				await logout(page);
			}

			await page.goto("/login");
			await expect(page.locator("form button[type='submit']")).toBeVisible();

			const identifier = page.getByLabel("Email or username");
			await identifier.fill("esap");
			await identifier.evaluate((input) => {
				if (!(input instanceof HTMLInputElement)) return;
				input.setSelectionRange(2, 2);
			});
			await identifier.pressSequentially("X");
			await expect(identifier).toHaveValue("esXap");
			await expect
				.poll(() =>
					identifier.evaluate((input) =>
						input instanceof HTMLInputElement
							? [input.selectionStart, input.selectionEnd]
							: [null, null],
					),
				)
				.toEqual([3, 3]);

			const password = page.locator("#password");
			await password.fill("secret");
			await password.evaluate((input) => {
				if (!(input instanceof HTMLInputElement)) return;
				input.setSelectionRange(3, 3);
			});
			await password.pressSequentially("X");
			await expect(password).toHaveValue("secXret");
			await expect
				.poll(() =>
					password.evaluate((input) =>
						input instanceof HTMLInputElement
							? [input.selectionStart, input.selectionEnd]
							: [null, null],
					),
				)
				.toEqual([4, 4]);
		});

		test("uses conditional passkey UI without a typed identifier", async ({
			page,
			request,
		}) => {
			if (!(await hasUsers(request))) {
				await setupAdmin(page);
				await logout(page);
			}

			await capturePasskeyGetCalls(page);
			await installPasskeyBrowserMock(page, {
				conditionalAvailable: true,
				resolveGetManually: true,
			});
			const passkeyRequests = await mockPasskeyLoginEndpoints(page, {
				expectStartPayload: (payload) =>
					expect(payload).toEqual({ conditional: true }),
			});
			await page.route("**/api/v1/auth/me", async (route) => {
				await route.fulfill({
					contentType: "application/json",
					status: 200,
					body: JSON.stringify({
						code: 0,
						data: {
							email: "admin@example.com",
							id: 1,
							preferences: {},
							role: "admin",
							status: "active",
							storage_quota: 0,
							storage_used: 0,
							username: "admin",
						},
						msg: "",
					}),
				});
			});

			await page.goto("/login");
			const identifier = page.getByLabel("Email or username");
			await expect(identifier).toHaveAttribute(
				"autocomplete",
				"username webauthn",
			);

			await expect
				.poll(() => passkeyRequests.startPayloads.length)
				.toBeGreaterThan(0);
			const calls = await readPasskeyGetCalls(page);
			expect(calls).toContainEqual({
				hasSignal: true,
				mediation: "conditional",
			});

			await resolvePendingPasskeyGet(page);
			await expect(page).toHaveURL(/\/$/);
			await expect
				.poll(() => passkeyRequests.finishPayloads.length)
				.toBeGreaterThan(0);
		});

		test("keeps the explicit passkey button as a discoverable-login fallback", async ({
			page,
			request,
		}) => {
			if (!(await hasUsers(request))) {
				await setupAdmin(page);
				await logout(page);
			}

			await capturePasskeyGetCalls(page);
			await installPasskeyBrowserMock(page, { conditionalAvailable: false });
			const passkeyRequests = await mockPasskeyLoginEndpoints(page, {
				expectStartPayload: (payload) => expect(payload).toEqual({}),
			});
			await page.route("**/api/v1/auth/me", async (route) => {
				await route.fulfill({
					contentType: "application/json",
					status: 200,
					body: JSON.stringify({
						code: 0,
						data: {
							email: "admin@example.com",
							id: 1,
							preferences: {},
							role: "admin",
							status: "active",
							storage_quota: 0,
							storage_used: 0,
							username: "admin",
						},
						msg: "",
					}),
				});
			});

			await page.goto("/login");
			await page.getByRole("button", { name: "Sign in with passkey" }).click();
			await expect(page).toHaveURL(/\/$/);

			expect(passkeyRequests.startPayloads).toEqual([{}]);
			expect(passkeyRequests.finishPayloads).toHaveLength(1);
			const calls = await readPasskeyGetCalls(page);
			expect(calls).toContainEqual({
				hasSignal: false,
				mediation: null,
			});
		});

		test("passes the typed identifier to explicit passkey login", async ({
			page,
			request,
		}) => {
			if (!(await hasUsers(request))) {
				await setupAdmin(page);
				await logout(page);
			}

			await capturePasskeyGetCalls(page);
			await installPasskeyBrowserMock(page, { conditionalAvailable: false });
			const passkeyRequests = await mockPasskeyLoginEndpoints(page, {
				expectStartPayload: (payload) =>
					expect(payload).toEqual({ identifier: "admin@example.com" }),
			});
			await page.route("**/api/v1/auth/me", async (route) => {
				await route.fulfill({
					contentType: "application/json",
					status: 200,
					body: JSON.stringify({
						code: 0,
						data: {
							email: "admin@example.com",
							id: 1,
							preferences: {},
							role: "admin",
							status: "active",
							storage_quota: 0,
							storage_used: 0,
							username: "admin",
						},
						msg: "",
					}),
				});
			});

			await page.goto("/login");
			await page.getByLabel("Email or username").fill("admin@example.com");
			await page.getByRole("button", { name: "Sign in with passkey" }).click();
			await expect(page).toHaveURL(/\/$/);

			expect(passkeyRequests.startPayloads).toEqual([
				{ identifier: "admin@example.com" },
			]);
			expect(passkeyRequests.finishPayloads).toHaveLength(1);
			const calls = await readPasskeyGetCalls(page);
			expect(calls).toContainEqual({
				hasSignal: false,
				mediation: null,
			});
		});
	});

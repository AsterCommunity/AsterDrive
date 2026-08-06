import { E2E_API_SUCCESS_CODE } from "./support/api-response";
import {
	configureInitialStorage,
	createInitialAdmin,
	ensureCurrentPublicSiteUrl,
	hasUsers,
	loginAsAdmin,
	logout,
	setupAdmin,
} from "./support/auth";
import { fileDropZone } from "./support/files";
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
		test.describe.configure({ retries: 0 });

		test("creates the initial admin, requires storage setup, and synchronizes completion across tabs", async ({
			context,
			page,
			request,
		}) => {
			await disablePasskeyBrowserSupport(page);
			expect(await hasUsers(request)).toBe(false);
			await createInitialAdmin(page);
			expect(await hasUsers(request)).toBe(true);

			await page.goto("/");
			await expect(page).toHaveURL(/\/setup\/storage$/);
			await page.goto("/admin/overview");
			await expect(page).toHaveURL(/\/setup\/storage$/);

			const waitingPage = await context.newPage();
			await waitingPage.goto("/setup/storage");
			await expect(
				waitingPage.getByRole("heading", {
					name: "Configure AsterDrive's first reliable storage",
				}),
			).toBeVisible();
			await expect(
				waitingPage.getByRole("button", { name: "Start storage setup" }),
			).toBeVisible();
			await expect(waitingPage.getByRole("dialog")).toHaveCount(0);

			await configureInitialStorage(page);
			await expect(waitingPage).toHaveURL(/\/$/, { timeout: 15_000 });
			await expect(fileDropZone(waitingPage)).toBeVisible();
			await waitingPage.close();
			await ensureCurrentPublicSiteUrl(page);

			await logout(page);
			await loginAsAdmin(page);
		});

		test("keeps the PWA workspace shell when cached authentication starts offline", async ({
			page,
			request,
		}) => {
			await disablePasskeyBrowserSupport(page);
			if (await hasUsers(request)) {
				await loginAsAdmin(page);
			} else {
				await setupAdmin(page);
			}

			await page.route("**/api/v1/auth/me**", (route) =>
				route.abort("internetdisconnected"),
			);
			await page.route("**/api/v1/auth/check", (route) =>
				route.abort("internetdisconnected"),
			);

			await page.reload();

			await expect(fileDropZone(page)).toBeVisible();
			await expect(page.getByText("Offline", { exact: true })).toBeVisible();
			await expect(
				page.getByRole("heading", {
					name: "Setup status could not be loaded",
				}),
			).toHaveCount(0);
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
			await identifier.focus();
			await identifier.evaluate((input) => {
				if (!(input instanceof HTMLInputElement)) return;
				input.setSelectionRange(2, 2);
			});
			await expect
				.poll(() =>
					identifier.evaluate((input) =>
						input instanceof HTMLInputElement
							? [input.selectionStart, input.selectionEnd]
							: [null, null],
					),
				)
				.toEqual([2, 2]);
			await page.keyboard.type("X");
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
			await password.focus();
			await password.evaluate((input) => {
				if (!(input instanceof HTMLInputElement)) return;
				input.setSelectionRange(3, 3);
			});
			await expect
				.poll(() =>
					password.evaluate((input) =>
						input instanceof HTMLInputElement
							? [input.selectionStart, input.selectionEnd]
							: [null, null],
					),
				)
				.toEqual([3, 3]);
			await page.keyboard.type("X");
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
						code: E2E_API_SUCCESS_CODE,
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
						code: E2E_API_SUCCESS_CODE,
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
						code: E2E_API_SUCCESS_CODE,
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

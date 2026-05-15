import { createTeamViaApi } from "./support/api";
import {
	authenticate,
	loginAsAdmin,
	loginWithCredentials,
	logout,
} from "./support/auth";
import { fileDropZone } from "./support/files";
import { uniqueAccountName, uniqueName } from "./support/fixtures";
import { apiJsonInPage, waitForApiCondition } from "./support/network";
import { expect, test } from "./support/test";
import {
	capturePasskeyGetCalls,
	disablePasskeyBrowserSupport,
	installPasskeyBrowserMock,
	mockPasskeyRegistrationEndpoints,
	readPasskeyCreateCalls,
} from "./support/webauthn";

test.describe
	.serial("Settings E2E", () => {
		test("updates personal profile, preferences, and password settings", async ({
			page,
			request,
		}) => {
			await disablePasskeyBrowserSupport(page);
			await authenticate(page, request);

			const username = uniqueAccountName("pwset");
			const email = `${username}@example.com`;
			const initialPassword = "Playwright123!";
			const updatedPassword = "Playwright456!";
			const displayName = `${username} Display`;

			await apiJsonInPage(page, "/api/v1/admin/users", {
				body: {
					email,
					password: initialPassword,
					username,
				},
				method: "POST",
				withCsrf: true,
			});

			await logout(page);
			await loginWithCredentials(page, email, initialPassword);

			await page.goto("/settings/profile");
			await expect(
				page.getByRole("tabpanel", { exact: true, name: "Profile" }),
			).toBeVisible({ timeout: 30_000 });
			await page
				.getByRole("textbox", { exact: true, name: "Display Name" })
				.fill(displayName);
			await page.getByRole("button", { exact: true, name: "Save" }).click();
			await expect(page.getByRole("button", { name: displayName })).toBeVisible(
				{
					timeout: 30_000,
				},
			);

			let me = await apiJsonInPage<{
				preferences?: {
					browser_open_mode?: string | null;
					color_preset?: string | null;
					storage_event_stream_enabled?: boolean | null;
					theme_mode?: string | null;
					view_mode?: string | null;
				} | null;
				profile: {
					display_name?: string | null;
				};
			}>(page, "/api/v1/auth/me");
			expect(me.profile.display_name).toBe(displayName);

			await page.goto("/settings/interface");
			await expect(
				page.getByRole("tabpanel", { exact: true, name: "Interface" }),
			).toBeVisible({ timeout: 30_000 });
			await page.getByRole("button", { exact: true, name: "Dark" }).click();
			await page
				.getByRole("button", { exact: true, name: "Grid View" })
				.click();
			await page
				.getByRole("button", { exact: true, name: "Double click" })
				.click();
			const storageEventSwitch = page.getByRole("switch", {
				exact: true,
				name: "Real-time file sync",
			});
			await storageEventSwitch.click();
			await expect(storageEventSwitch).not.toBeChecked();

			me = await waitForApiCondition<typeof me>(
				page,
				"/api/v1/auth/me",
				(data) =>
					data.preferences?.theme_mode === "dark" &&
					data.preferences?.view_mode === "grid" &&
					data.preferences?.browser_open_mode === "double_click" &&
					data.preferences?.storage_event_stream_enabled === false,
			);
			expect(me.preferences?.theme_mode).toBe("dark");
			expect(me.preferences?.view_mode).toBe("grid");
			expect(me.preferences?.browser_open_mode).toBe("double_click");
			expect(me.preferences?.storage_event_stream_enabled).toBe(false);

			await page.goto("/settings/security");
			await expect(
				page.getByRole("tabpanel", { exact: true, name: "Security" }),
			).toBeVisible({ timeout: 30_000 });
			await expect(
				page.getByText("Current device", { exact: true }),
			).toBeVisible({
				timeout: 30_000,
			});
			await page
				.getByLabel("Current password", { exact: true })
				.fill(initialPassword);
			await page
				.getByLabel("New password", { exact: true })
				.fill(updatedPassword);
			await page
				.getByLabel("Confirm new password", { exact: true })
				.fill(updatedPassword);
			const passwordForm = page
				.getByRole("heading", {
					exact: true,
					name: "Password",
				})
				.locator("xpath=ancestor::form[1]");
			await passwordForm.getByRole("button", { name: /^save$/i }).click();
			await expect(
				page.getByLabel("Current password", { exact: true }),
			).toHaveValue("", { timeout: 30_000 });

			await logout(page, displayName);
			await loginWithCredentials(page, email, updatedPassword);
			await expect(fileDropZone(page)).toBeVisible({ timeout: 30_000 });

			await logout(page, displayName);
			await loginAsAdmin(page);
		});

		test("opens team settings and team management sections", async ({
			page,
			request,
		}) => {
			await disablePasskeyBrowserSupport(page);
			await authenticate(page, request);

			const teamName = uniqueName("pw-settings-team");
			const team = await createTeamViaApi(
				page,
				teamName,
				"Team created for settings coverage",
			);

			await page.goto("/settings/teams");
			const teamsPanel = page.getByRole("tabpanel", {
				exact: true,
				name: "Teams",
			});
			await expect(teamsPanel).toBeVisible({ timeout: 30_000 });

			const teamCard = teamsPanel
				.getByText(teamName, { exact: true })
				.locator(
					"xpath=ancestor::div[contains(concat(' ', normalize-space(@class), ' '), ' rounded-xl ')][1]",
				);
			await expect(teamCard).toBeVisible({ timeout: 30_000 });
			await teamCard
				.getByRole("button", { exact: true, name: "Manage" })
				.click();

			await expect(page).toHaveURL(
				new RegExp(`/settings/teams/${team.id}/overview$`),
			);
			await expect(page.locator("#team-manage-name")).toHaveValue(teamName, {
				timeout: 30_000,
			});

			await page.getByRole("tab", { exact: true, name: "Members" }).click();
			await expect(page).toHaveURL(
				new RegExp(`/settings/teams/${team.id}/members$`),
			);
			await expect(
				page.getByRole("row").filter({ hasText: "admin@example.com" }).first(),
			).toBeVisible({ timeout: 30_000 });

			await page.getByRole("tab", { exact: true, name: "Team audit" }).click();
			await expect(page).toHaveURL(
				new RegExp(`/settings/teams/${team.id}/audit$`),
			);
			await expect(page.getByText("Created team")).toBeVisible({
				timeout: 30_000,
			});

			await page.getByRole("tab", { exact: true, name: "Danger zone" }).click();
			await expect(page).toHaveURL(
				new RegExp(`/settings/teams/${team.id}/danger$`),
			);
			await expect(
				page.getByRole("heading", { exact: true, name: "Danger zone" }),
			).toBeVisible();

			await page
				.getByRole("button", { exact: true, name: "Open workspace" })
				.click();
			await expect(page).toHaveURL(new RegExp(`/teams/${team.id}$`));
		});

		test("manages passkeys from security settings", async ({
			page,
			request,
		}) => {
			await disablePasskeyBrowserSupport(page);
			await authenticate(page, request);
			await capturePasskeyGetCalls(page);
			await installPasskeyBrowserMock(page, { supportCreate: true });
			const passkeyRequests = await mockPasskeyRegistrationEndpoints(page);

			await page.goto("/settings/security");
			const securityPanel = page.getByRole("tabpanel", {
				exact: true,
				name: "Security",
			});
			await expect(securityPanel).toBeVisible({ timeout: 30_000 });

			await page.getByLabel("New passkey name").fill("Laptop");
			await page
				.getByRole("button", { exact: true, name: "Add passkey" })
				.click();
			await expect(page.getByText("Laptop", { exact: true })).toBeVisible({
				timeout: 30_000,
			});
			expect(passkeyRequests.startPayloads).toEqual([{ name: "Laptop" }]);
			expect(passkeyRequests.finishPayloads).toHaveLength(1);
			const createCalls = await readPasskeyCreateCalls(page);
			expect(createCalls).toContainEqual({ hasPublicKey: true });

			await page.getByRole("button", { exact: true, name: "Rename" }).click();
			await page.getByLabel("Edit passkey name").fill("Phone");
			await page.getByRole("button", { exact: true, name: "Save" }).click();
			await expect(page.getByText("Phone", { exact: true })).toBeVisible({
				timeout: 30_000,
			});
			expect(passkeyRequests.patchRequests).toHaveLength(1);
			expect(passkeyRequests.patchRequests[0]).toMatchObject({
				body: { name: "Phone" },
				id: 7,
				method: "PATCH",
			});
			expect(passkeyRequests.patchRequests[0]?.url).toContain(
				"/api/v1/auth/passkeys/7",
			);

			await page.getByRole("button", { exact: true, name: "Delete" }).click();
			const dialog = page.getByRole("alertdialog", {
				exact: true,
				name: "Delete passkey",
			});
			await expect(dialog).toBeVisible();
			await dialog.getByRole("button", { exact: true, name: "Delete" }).click();
			await expect(page.getByText("No passkeys yet")).toBeVisible({
				timeout: 30_000,
			});
			expect(passkeyRequests.deleteRequests).toHaveLength(1);
			expect(passkeyRequests.deleteRequests[0]).toMatchObject({
				id: 7,
				method: "DELETE",
			});
			expect(passkeyRequests.deleteRequests[0]?.url).toContain(
				"/api/v1/auth/passkeys/7",
			);
		});
	});

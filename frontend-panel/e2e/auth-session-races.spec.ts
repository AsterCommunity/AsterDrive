import { authenticate, loginAsAdmin } from "./support/auth";
import { fileDropZone } from "./support/files";
import { ADMIN } from "./support/fixtures";
import { expect, test } from "./support/test";

// These use real signed cookies and writer-backed sessions. Delaying a JS callback alone would
// miss the bug: the browser applies Set-Cookie independently of the Zustand generation guard.
for (const source of ["same tab", "other tab"] as const) {
	test(`logout waits for a delayed refresh response from ${source}`, async ({
		page,
		context,
		request,
		browser,
	}) => {
		await authenticate(page, request);
		const independentContext = await browser.newContext({
			baseURL: new URL(page.url()).origin,
			locale: "en-US",
		});
		const independentPage = await independentContext.newPage();
		try {
			await independentPage.goto(page.url());
			await loginAsAdmin(independentPage);
			const logoutPage = source === "same tab" ? page : await context.newPage();
			if (logoutPage !== page) {
				await logoutPage.goto("/");
				await expect(fileDropZone(logoutPage)).toBeVisible();
			}
			let releaseResponse!: () => void;
			const heldResponse = new Promise<void>((resolve) => {
				releaseResponse = resolve;
			});
			let refreshCommitted!: () => void;
			const committed = new Promise<void>((resolve) => {
				refreshCommitted = resolve;
			});
			let logoutRequests = 0;
			const originalRefresh = (await context.cookies()).find(
				(cookie) => cookie.name === "aster_refresh",
			)?.value;
			let rotatedCookies: Awaited<ReturnType<typeof context.cookies>> = [];
			context.on("request", (req) => {
				if (req.url().endsWith("/auth/logout")) logoutRequests += 1;
			});
			await page.route("**/api/v1/auth/me**", async (route) => {
				const response = await route.fetch();
				const payload = await response.json();
				if (payload.data)
					payload.data.access_token_expires_at =
						Math.floor(Date.now() / 1000) + 10;
				await route.fulfill({ response, json: payload });
			});
			await page.route("**/api/v1/auth/refresh", async (route) => {
				const response = await route.fetch();
				expect(response.status()).toBe(200);
				rotatedCookies = await context.cookies();
				expect(
					rotatedCookies.find((cookie) => cookie.name === "aster_refresh")
						?.value,
				).not.toBe(originalRefresh);
				refreshCommitted();
				await heldResponse;
				await route.fulfill({ response });
			});
			await page.reload();
			await committed;
			await logoutPage.getByRole("button", { name: ADMIN.username }).click();
			await logoutPage
				.getByRole("button", { name: "Logout", exact: true })
				.click();
			await expect
				.poll(() =>
					logoutPage.evaluate(() => localStorage.getItem("aster-auth-logout")),
				)
				.not.toBeNull();
			expect(logoutRequests).toBe(0);
			releaseResponse();
			await expect.poll(() => logoutRequests).toBe(1);
			await expect(logoutPage).toHaveURL(/\/login$/);
			await page.unroute("**/api/v1/auth/me**");
			await page.unroute("**/api/v1/auth/refresh");
			await expect
				.poll(async () =>
					(await context.request.get("/api/v1/auth/me")).status(),
				)
				.toBe(401);
			// Reinstall the real rotated pair to model a further late cookie delivery.
			// A missing CSRF cookie would only prove the CSRF guard, so restore its matching value too.
			await context.addCookies(rotatedCookies);
			expect((await context.request.get("/api/v1/auth/me")).status()).toBe(401);
			const csrf =
				(await context.cookies()).find((cookie) => cookie.name === "aster_csrf")
					?.value ?? "";
			expect(
				(
					await context.request.post("/api/v1/auth/refresh", {
						headers: {
							"X-CSRF-Token": csrf,
							Origin: new URL(page.url()).origin,
						},
					})
				).status(),
			).toBe(401);
			await page.reload();
			await expect(page).toHaveURL(/\/login$/);
			expect(
				(
					await independentContext.request.get(
						new URL("/api/v1/auth/me", independentPage.url()).toString(),
					)
				).status(),
			).toBe(200);
		} finally {
			await independentContext.close();
		}
	});
}

import type { Page } from "@playwright/test";
import { uploadFileViaApi } from "./support/api";
import { authenticate } from "./support/auth";
import {
	chooseOpenMethodIfPrompted,
	closeActiveDialog,
	fileNameCell,
} from "./support/files";
import { uniqueName } from "./support/fixtures";
import { apiJsonInPage } from "./support/network";
import { expect, test } from "./support/test";

async function selectViewer(page: Page, filename: string, label: string) {
	await fileNameCell(page, filename).click();
	if (
		!(await page
			.getByRole("dialog", { name: "Choose how to open this file" })
			.isVisible())
	) {
		await page
			.getByRole("button", {
				name: "Choose how to open this file",
				exact: true,
			})
			.click();
	}
	await chooseOpenMethodIfPrompted(page, label, 2000);
}

test("administrator embed and WOPI integrations retain scripts, POST launch and window behavior", async ({
	page,
	context,
	request,
}) => {
	await authenticate(page, request);
	const config = await apiJsonInPage<{ version: number; apps: unknown[] }>(
		page,
		"/api/v1/public/preview-apps",
	);
	const apps = [
		{
			key: "custom.compat_local",
			provider: "url_template",
			icon: "Globe",
			enabled: true,
			labels: { en: "Compat Local Embed" },
			extensions: ["txt"],
			config: {
				mode: "iframe",
				url_template: "/compat/embed?name={{file_name}}",
			},
		},
		{
			key: "custom.compat_external",
			provider: "url_template",
			icon: "Globe",
			enabled: true,
			labels: { en: "Compat External Embed" },
			extensions: ["txt"],
			config: {
				mode: "iframe",
				url_template: "https://viewer.fixture.invalid/embed?name={{file_name}}",
				allowed_origins: ["https://viewer.fixture.invalid"],
			},
		},
		{
			key: "custom.compat_office",
			provider: "wopi",
			icon: "Globe",
			enabled: true,
			labels: { en: "Compat Office" },
			extensions: ["txt"],
			config: {
				mode: "iframe",
				action_url: "https://office.fixture.invalid/editor",
				allowed_origins: ["https://office.fixture.invalid"],
			},
		},
	];
	await apiJsonInPage(page, "/api/v1/admin/config/frontend_preview_apps_json", {
		method: "PUT",
		withCsrf: true,
		body: {
			value: JSON.stringify({ ...config, apps: [...config.apps, ...apps] }),
		},
	});
	const filename = `${uniqueName("compat-document")}.txt`;
	await uploadFileViaApi(page, "/api/v1", {
		name: filename,
		mimeType: "text/plain",
		buffer: Buffer.from("Office compatibility"),
	});
	const embedBody =
		'<!doctype html><p id="active">loading</p><script>document.getElementById("active").textContent="Configured application script works"</script><form><input name="q" value="ok"></form>';
	await context.route("**/compat/embed?**", (route) =>
		route.fulfill({ contentType: "text/html", body: embedBody }),
	);
	await context.route("https://viewer.fixture.invalid/embed?**", (route) =>
		route.fulfill({ contentType: "text/html", body: embedBody }),
	);
	let launchMode: "iframe" | "new_tab" = "iframe";
	await page.route("**/api/v1/files/*/wopi/open", (route) =>
		route.fulfill({
			json: {
				code: "success",
				msg: "",
				data: {
					access_token: "compat-file-scoped-token",
					access_token_ttl: Date.now() + 60000,
					action_url: "https://office.fixture.invalid/editor",
					form_fields: { custom_field: "preserved" },
					mode: launchMode,
				},
			},
		}),
	);
	const posts: URLSearchParams[] = [];
	await context.route(
		"https://office.fixture.invalid/editor",
		async (route) => {
			expect(route.request().method()).toBe("POST");
			expect(route.request().headers().origin).toBe(new URL(page.url()).origin);
			expect(route.request().headers().referer).toContain(
				new URL(page.url()).origin,
			);
			const fields = new URLSearchParams(route.request().postData() ?? "");
			posts.push(fields);
			await route.fulfill({
				contentType: "text/html",
				body: '<!doctype html><p id="office">loading</p><script>localStorage.setItem("office-storage","works");document.getElementById("office").textContent="Office scripts and storage work"</script><a href="https://office.fixture.invalid/download" download>Download document</a>',
			});
		},
	);
	try {
		await page.reload();
		for (const label of ["Compat Local Embed", "Compat External Embed"]) {
			await selectViewer(page, filename, label);
			const iframe = page.frameLocator(`iframe[title="${label}"]`);
			await expect(iframe.locator("#active")).toHaveText(
				"Configured application script works",
			);
			await expect(iframe.locator("input[name=q]")).toHaveValue("ok");
			await closeActiveDialog(page);
		}
		await selectViewer(page, filename, "Compat Office");
		await expect(
			page.frameLocator('iframe[title="Compat Office"]').locator("#office"),
		).toHaveText("Office scripts and storage work");
		const embedded = page.locator('iframe[title="Compat Office"]');
		await expect(embedded).toHaveAttribute("sandbox", /allow-same-origin/);
		await expect(embedded).toHaveAttribute("sandbox", /allow-downloads/);
		await expect(embedded).toHaveAttribute("allow", /clipboard-write/);
		const previousPages = new Set(context.pages());
		await page
			.getByRole("button", { name: "Open in Compat Office", exact: true })
			.click();
		await expect
			.poll(
				() =>
					context
						.pages()
						.filter(
							(candidate) =>
								!previousPages.has(candidate) &&
								candidate
									.url()
									.startsWith("https://office.fixture.invalid/editor"),
						).length,
			)
			.toBe(1);
		const popup = context
			.pages()
			.find(
				(candidate) =>
					!previousPages.has(candidate) &&
					candidate.url().startsWith("https://office.fixture.invalid/editor"),
			);
		if (!popup) throw new Error("WOPI application did not open");
		try {
			await expect(popup.locator("#office")).toHaveText(
				"Office scripts and storage work",
			);
			expect(await popup.evaluate(() => window.opener === null)).toBe(true);
			expect(
				context.pages().filter((candidate) => !previousPages.has(candidate)),
			).toHaveLength(1);
		} finally {
			for (const candidate of context.pages())
				if (!previousPages.has(candidate)) await candidate.close();
		}

		await closeActiveDialog(page);
		launchMode = "new_tab";
		await selectViewer(page, filename, "Compat Office");
		await expect(page.locator('iframe[title="Compat Office"]')).toHaveCount(0);
		const externalPromise = context.waitForEvent("page");
		await page
			.getByRole("button", { name: "Open in Compat Office", exact: true })
			.click();
		const external = await externalPromise;
		try {
			await expect(external.locator("#office")).toHaveText(
				"Office scripts and storage work",
			);
			expect(await external.evaluate(() => window.opener === null)).toBe(true);
		} finally {
			await external.close();
		}

		expect(posts).toHaveLength(3);
		for (const fields of posts) {
			expect(fields.get("access_token")).toBe("compat-file-scoped-token");
			expect(fields.get("custom_field")).toBe("preserved");
			expect(Number(fields.get("access_token_ttl"))).toBeGreaterThan(
				Date.now(),
			);
		}
	} finally {
		await apiJsonInPage(
			page,
			"/api/v1/admin/config/frontend_preview_apps_json",
			{
				method: "PUT",
				withCsrf: true,
				body: { value: JSON.stringify(config) },
			},
		);
	}
});

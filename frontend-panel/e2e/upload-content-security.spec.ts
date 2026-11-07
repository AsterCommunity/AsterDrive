import { uploadFileViaApi } from "./support/api";
import { authenticate } from "./support/auth";
import {
	chooseOpenMethodIfPrompted,
	expectImagePreview,
	fileNameCell,
} from "./support/files";
import { PDF_FILE, uniqueAccountName, uniqueName } from "./support/fixtures";
import { apiJsonInPage, normalizeWebdavPrefix } from "./support/network";
import { expect, test } from "./support/test";

const SVG =
	'<svg xmlns="http://www.w3.org/2000/svg" width="160" height="80"><script>localStorage.setItem("preview-xss-probe",location.origin)</script><rect width="160" height="80" fill="green"/><circle cx="15" cy="40" r="8"><animate attributeName="cx" values="15;145;15" dur="2s" repeatCount="indefinite"/></circle></svg>';

test("uploaded SVG remains an image without a navigable application-origin script", async ({
	page,
	request,
	context,
}) => {
	await authenticate(page, request);
	const name = `${uniqueName("active-svg")}.svg`;
	const originalFile = await uploadFileViaApi(page, "/api/v1", {
		name,
		mimeType: "image/svg+xml",
		buffer: Buffer.from(SVG),
	});
	await page.reload();
	await expectImagePreview(page, name);
	const img = page.getByRole("dialog").getByRole("img", { name });
	await expect
		.poll(() => img.evaluate((el) => (el as HTMLImageElement).naturalWidth))
		.toBe(160);
	const originalDownload = await context.request.get(
		`/api/v1/files/${originalFile.id}/download`,
	);
	expect((await originalDownload.body()).equals(Buffer.from(SVG))).toBe(true);
	const source = await img.getAttribute("src");
	expect(source).toBeTruthy();
	const documentPage = await context.newPage();
	try {
		await documentPage.goto(source as string);
		expect(
			await page.evaluate(() => localStorage.getItem("preview-xss-probe")),
		).toBeNull();
		const mainOriginReachable = await documentPage.evaluate(() => {
			try {
				return localStorage.getItem("preview-xss-probe") !== null;
			} catch {
				return false;
			}
		});
		expect(mainOriginReachable).toBe(false);
	} finally {
		await documentPage.close();
	}
});

test("HTML named as a PDF cannot execute from the PDF toolbar", async ({
	page,
	request,
	context,
}) => {
	await authenticate(page, request);
	const name = `${uniqueName("fake-document")}.pdf`;
	await uploadFileViaApi(page, "/api/v1", {
		name,
		mimeType: "text/html",
		buffer: Buffer.concat([
			PDF_FILE.buffer,
			Buffer.from(
				'<script>localStorage.setItem("pdf-xss-probe",location.origin)</script>',
			),
		]),
	});
	await page.reload();
	await fileNameCell(page, name).click();
	await chooseOpenMethodIfPrompted(page, "PDF preview", 2000);
	const openButton = page.getByRole("button", {
		name: "Open in new tab",
		exact: true,
	});
	await expect(openButton.first()).toBeEnabled();
	// Native PDF handling differs between headed/headless Chromium. Capture the URL passed to
	// the real window.open, then independently verify its MIME rather than treating about:blank
	// (or the native viewer's extension page) as the file response.
	await page.evaluate(() => {
		const original = window.open;
		window.open = (url, target, features) => {
			document.documentElement.dataset.pdfNavigation = String(url);
			return original.call(window, url, target, features);
		};
	});
	const popupPromise = context.waitForEvent("page");
	await openButton.first().click();
	const popup = await popupPromise;
	try {
		// Chromium's native PDF viewer does not emit a regular DOMContentLoaded for its outer tab.
		await popup.waitForLoadState("networkidle");
		expect(
			await page.evaluate(
				async (url) => (await (await fetch(url as string)).blob()).type,
				await page.locator("html").getAttribute("data-pdf-navigation"),
			),
		).toBe("application/pdf");
		expect(
			await page.evaluate(() => localStorage.getItem("pdf-xss-probe")),
		).toBeNull();
	} finally {
		await popup.close();
	}
});

test("WebDAV uploaded HTML cannot execute with the application session", async ({
	page,
	request,
	context,
}) => {
	await authenticate(page, request);
	const settings = await apiJsonInPage<{ prefix: string }>(
		page,
		"/api/v1/webdav-accounts/settings",
	);
	const name = `${uniqueName("dav-active")}.html`;
	await uploadFileViaApi(page, "/api/v1", {
		name,
		mimeType: "text/html",
		buffer: Buffer.from(
			'<!doctype html><script>localStorage.setItem("dav-xss-probe",location.origin)</script><p>Document</p>',
		),
	});
	const username = uniqueAccountName("securitydav");
	const password = "SecurityFixture!3841";
	await apiJsonInPage(page, "/api/v1/webdav-accounts", {
		method: "POST",
		withCsrf: true,
		body: { username, password },
	});
	const authorization = `Basic ${Buffer.from(`${username}:${password}`).toString("base64")}`;
	const path = `${normalizeWebdavPrefix(settings.prefix)}/${name}`;
	const viewer = await context.newPage();
	await viewer.setExtraHTTPHeaders({ Authorization: authorization });
	try {
		const response = await viewer.goto(path);
		expect(response?.status()).toBe(200);
		expect(
			await page.evaluate(() => localStorage.getItem("dav-xss-probe")),
		).toBeNull();
	} finally {
		await viewer.close();
	}
});

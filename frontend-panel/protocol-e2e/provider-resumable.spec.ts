import { createServer, type IncomingHttpHeaders, type Server } from "node:http";
import { expect, test } from "@playwright/test";

type BrowserResult = {
	offset?: number;
	error?: string;
	retryable?: boolean;
	aborted?: boolean;
	status?: number;
};
declare global {
	interface Window {
		uploadFragment: (
			url: string,
			start: number,
			total: number,
			content: string,
		) => Promise<BrowserResult>;
		activeUpload?: XMLHttpRequest;
	}
}

let server: Server;
let origin: string;
let seen: { method: string; headers: IncomingHttpHeaders; body: string }[] = [];

test.beforeAll(async () => {
	server = createServer(async (request, response) => {
		const chunks: Buffer[] = [];
		for await (const chunk of request) chunks.push(Buffer.from(chunk));
		seen.push({
			method: request.method ?? "",
			headers: request.headers,
			body: Buffer.concat(chunks).toString(),
		});
		const path = request.url ?? "";
		if (!path.includes("deny-cors"))
			response.setHeader(
				"Access-Control-Allow-Origin",
				"http://127.0.0.1:3347",
			);
		response.setHeader("Access-Control-Allow-Methods", "PUT, OPTIONS");
		response.setHeader(
			"Access-Control-Allow-Headers",
			"Content-Type, Content-Range",
		);
		response.setHeader("Cache-Control", "no-store");
		if (request.method === "OPTIONS") {
			response.writeHead(204).end();
			return;
		}
		if (path.includes("slow")) return;
		if (path.includes("final")) {
			response
				.writeHead(200, { "Content-Type": "application/json" })
				.end('{"id":"owned"}');
			return;
		}
		if (!path.includes("hidden-range"))
			response.setHeader("Access-Control-Expose-Headers", "Range");
		response.writeHead(308, { Range: "bytes=0-3" }).end();
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
	const address = server.address();
	if (!address || typeof address === "string")
		throw new Error("missing provider mock address");
	origin = `http://127.0.0.1:${address.port}`;
});
test.afterAll(async () => {
	server.closeAllConnections();
	await new Promise<void>((resolve, reject) =>
		server.close((error) => (error ? reject(error) : resolve())),
	);
});
test.beforeEach(async ({ page, context }) => {
	seen = [];
	await context.addCookies([
		{ url: origin, name: "admin_cookie", value: "must-not-leak" },
	]);
	await page.goto("/protocol-e2e/fixtures/provider-upload.html");
	await page.waitForFunction(() => typeof window.uploadFragment === "function");
});

test("cross-origin PUT accepts 308 Range and final 200 with no cookies or authorization", async ({
	page,
}) => {
	expect(
		await page.evaluate(
			(url) => window.uploadFragment(`${url}/provider`, 0, 8, "abcd"),
			origin,
		),
	).toEqual({ offset: 4 });
	expect(
		await page.evaluate(
			(url) => window.uploadFragment(`${url}/final`, 4, 8, "efgh"),
			origin,
		),
	).toEqual({ offset: 8 });
	const preflight = seen.find((request) => request.method === "OPTIONS");
	expect(preflight?.headers["access-control-request-headers"]).toContain(
		"content-range",
	);
	const puts = seen.filter((request) => request.method === "PUT");
	expect(puts.map((request) => request.headers["content-range"])).toEqual([
		"bytes 0-3/8",
		"bytes 4-7/8",
	]);
	expect(puts.map((request) => request.body)).toEqual(["abcd", "efgh"]);
	for (const request of puts) {
		expect(request.headers.authorization).toBeUndefined();
		expect(request.headers.cookie).toBeUndefined();
		expect(request.headers["x-csrf-token"]).toBeUndefined();
	}
});

test("a CORS-hidden Range requests authenticated backend reconciliation", async ({
	page,
}) => {
	const result = await page.evaluate(
		(url) => window.uploadFragment(`${url}/hidden-range`, 0, 8, "abcd"),
		origin,
	);
	expect(result).toMatchObject({ status: 308, retryable: true });
	expect(result.offset).toBeUndefined();
});

test("failed CORS preflight remains a retryable network failure", async ({
	page,
}) => {
	const result = await page.evaluate(
		(url) => window.uploadFragment(`${url}/deny-cors`, 0, 8, "abcd"),
		origin,
	);
	expect(result).toMatchObject({ retryable: true });
	expect(seen.some((request) => request.method === "PUT")).toBe(false);
});

test("cancellation aborts the in-flight browser request", async ({ page }) => {
	const running = page.evaluate(
		(url) => window.uploadFragment(`${url}/slow`, 0, 8, "abcd"),
		origin,
	);
	await expect
		.poll(() => seen.some((request) => request.method === "PUT"))
		.toBe(true);
	await page.evaluate(() => window.activeUpload?.abort());
	expect(await running).toMatchObject({ aborted: true, retryable: false });
});

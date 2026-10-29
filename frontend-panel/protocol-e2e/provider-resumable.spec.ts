import { createServer, type IncomingHttpHeaders, type Server } from "node:http";
import { expect, test } from "@playwright/test";
import type { BrowserUploadStep } from "../src/types/api";

type BrowserResult = {
	headers?: Record<string, string>;
	body?: string;
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
		runCoordinatedUpload: (
			initial: BrowserUploadStep,
			controlUrl: string,
		) => Promise<void>;
	}
}

let coordinatedReports: unknown[] = [];
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
		response.setHeader(
			"Access-Control-Allow-Methods",
			"PUT, PATCH, POST, OPTIONS",
		);
		response.setHeader(
			"Access-Control-Allow-Headers",
			"Content-Type, Content-Range, X-Plugin-Protocol",
		);
		response.setHeader("Cache-Control", "no-store");
		if (request.method === "OPTIONS") {
			response.writeHead(204).end();
			return;
		}
		if (path.includes("coordinate")) {
			coordinatedReports.push(JSON.parse(Buffer.concat(chunks).toString()));
			response
				.writeHead(200, { "Content-Type": "application/json" })
				.end(
					JSON.stringify(
						pluginStep(
							coordinatedReports.filter(
								(report) => (report as { request_id?: string }).request_id,
							).length * 4,
						),
					),
				);
			return;
		}
		if (path.includes("plugin")) {
			response.setHeader("Access-Control-Expose-Headers", "X-Plugin-Ack");
			response
				.writeHead(299, { "X-Plugin-Ack": "opaque-ack" })
				.end("custom-protocol-body");
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
	coordinatedReports = [];
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
	).toMatchObject({ status: 308, headers: { range: "bytes=0-3" } });
	expect(
		await page.evaluate(
			(url) => window.uploadFragment(`${url}/final`, 4, 8, "efgh"),
			origin,
		),
	).toMatchObject({ status: 200, body: '{"id":"owned"}' });
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
	expect(result).toMatchObject({ status: 308, headers: {} });
});

test("failed CORS preflight remains a retryable network failure", async ({
	page,
}) => {
	const result = await page.evaluate(
		(url) => window.uploadFragment(`${url}/deny-cors`, 0, 8, "abcd"),
		origin,
	);
	expect(result).toMatchObject({ status: 0, headers: {} });
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

test("an unknown plugin protocol runs through real CORS requests and backend coordination", async ({
	page,
}) => {
	await page.evaluate(
		({ initial, controlUrl }) =>
			window.runCoordinatedUpload(initial, controlUrl),
		{ initial: pluginStep(0), controlUrl: `${origin}/coordinate` },
	);
	const reports = coordinatedReports;
	expect(reports).toHaveLength(3);
	expect(reports[0]).toEqual({});
	expect(reports[1]).toMatchObject({
		request_id: "id-0",
		observation: {
			status: 299,
			headers: { "x-plugin-ack": "opaque-ack" },
			body: "custom-protocol-body",
		},
	});
	expect(seen.filter((r) => r.method === "PATCH").map((r) => r.body)).toEqual([
		"abcd",
		"efgh",
	]);
	expect(seen.some((r) => r.method === "OPTIONS")).toBe(true);
	for (const request of seen) {
		expect(request.headers.cookie).toBeUndefined();
		expect(request.headers.authorization).toBeUndefined();
	}
});

function pluginStep(offset: number): BrowserUploadStep {
	return {
		executor_version: 1,
		protocol: { id: "external.plugin.upload", version: 42 },
		confirmed_offset: offset,
		progress_revision: offset,
		provider_completed: offset === 8,
		instruction:
			offset === 8
				? null
				: {
						request_id: `id-${offset}`,
						timeout_ms: 60000,
						response_body_limit: 65536,
						request: {
							url: `${origin}/plugin`,
							method: "PATCH",
							headers: { "X-Plugin-Protocol": "42" },
							body: { start: offset, end: offset + 4 },
							response_headers: ["X-Plugin-Ack"],
						},
					},
	};
}

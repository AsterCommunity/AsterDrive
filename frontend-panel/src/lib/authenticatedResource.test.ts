import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	cancelBody: vi.fn(),
	ensureFreshSession: vi.fn(),
	expiresAt: Date.now() + 60_000,
	fetch: vi.fn(),
	loggerError: vi.fn(),
	refreshToken: vi.fn(),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: {
		getState: () => ({
			expiresAt: mockState.expiresAt,
			ensureFreshSession: mockState.ensureFreshSession,
			refreshToken: mockState.refreshToken,
		}),
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		error: (...args: unknown[]) => mockState.loggerError(...args),
	},
}));

function mockProbeStatus(status: number) {
	mockState.fetch.mockResolvedValue({
		body: {
			cancel: mockState.cancelBody,
		},
		status,
	});
}

function mockProbeResponse(status: number, events?: string[]) {
	return {
		body: {
			cancel: vi.fn(async () => {
				events?.push(`cancel:${status}`);
				await mockState.cancelBody();
			}),
		},
		status,
	};
}

describe("prepareAuthenticatedResource", () => {
	beforeEach(() => {
		vi.resetModules();
		mockState.cancelBody.mockReset();
		mockState.cancelBody.mockResolvedValue(undefined);
		mockState.ensureFreshSession.mockReset();
		mockState.ensureFreshSession.mockResolvedValue(undefined);
		mockState.expiresAt = Date.now() + 60_000;
		mockState.fetch.mockReset();
		mockProbeStatus(206);
		mockState.loggerError.mockReset();
		mockState.refreshToken.mockReset();
		mockState.refreshToken.mockResolvedValue(undefined);
		vi.stubGlobal("fetch", mockState.fetch);
	});

	it("prepares protected API resources through the shared auth refresh path", async () => {
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await prepareAuthenticatedResource("/files/7/download");

		expect(mockState.ensureFreshSession).toHaveBeenCalledTimes(1);
		expect(mockState.fetch).toHaveBeenCalledWith(
			"/api/v1/files/7/download",
			expect.objectContaining({
				credentials: "include",
				headers: {
					Range: "bytes=0-0",
				},
			}),
		);
		expect(mockState.cancelBody).toHaveBeenCalledTimes(1);
	});

	it("accepts 416 range responses as a successful probe", async () => {
		mockProbeStatus(416);
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await prepareAuthenticatedResource("/files/7/download");

		expect(mockState.cancelBody).toHaveBeenCalledTimes(1);
	});

	it.each([
		"/s/share-token/download",
		"/api/v1/s/share-token/stream/session-token/video.mp4",
		"/d/direct-token/file.mp4",
		"/pv/preview-token/file.mp4",
		"https://cdn.example/file.mp4",
		"blob:http://localhost/file",
	])("skips public or already external resource %s", async (path) => {
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await prepareAuthenticatedResource(path);

		expect(mockState.ensureFreshSession).not.toHaveBeenCalled();
		expect(mockState.fetch).not.toHaveBeenCalled();
	});

	it("refreshes once and retries when the probe sees a missing access token", async () => {
		const events: string[] = [];
		mockState.fetch
			.mockImplementationOnce(async () => {
				events.push("probe:401");
				return mockProbeResponse(401, events);
			})
			.mockImplementationOnce(async () => {
				events.push("probe:206");
				return mockProbeResponse(206, events);
			});
		mockState.refreshToken.mockImplementationOnce(async () => {
			events.push("refresh");
			mockState.expiresAt += 900_000;
		});
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await prepareAuthenticatedResource("/files/7/download");

		expect(events).toEqual([
			"probe:401",
			"cancel:401",
			"refresh",
			"probe:206",
			"cancel:206",
		]);
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		expect(mockState.fetch).toHaveBeenNthCalledWith(
			1,
			"/api/v1/files/7/download",
			expect.objectContaining({
				credentials: "include",
				headers: { Range: "bytes=0-0" },
			}),
		);
		expect(mockState.fetch).toHaveBeenNthCalledWith(
			2,
			"/api/v1/files/7/download",
			expect.objectContaining({
				credentials: "include",
				headers: { Range: "bytes=0-0" },
			}),
		);
		expect(mockState.cancelBody).toHaveBeenCalledTimes(2);
	});

	it("passes abort signals through the initial and retried probe", async () => {
		const controller = new AbortController();
		mockState.fetch
			.mockResolvedValueOnce(mockProbeResponse(401))
			.mockResolvedValueOnce(mockProbeResponse(206));
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await prepareAuthenticatedResource("/files/7/download", {
			signal: controller.signal,
		});

		expect(mockState.fetch).toHaveBeenNthCalledWith(
			1,
			"/api/v1/files/7/download",
			expect.objectContaining({ signal: controller.signal }),
		);
		expect(mockState.fetch).toHaveBeenNthCalledWith(
			2,
			"/api/v1/files/7/download",
			expect.objectContaining({ signal: controller.signal }),
		);
	});

	it("does not start the retry probe when aborted after refresh", async () => {
		const controller = new AbortController();
		mockState.fetch.mockResolvedValueOnce(mockProbeResponse(401));
		mockState.refreshToken.mockImplementationOnce(async () => {
			controller.abort();
		});
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await expect(
			prepareAuthenticatedResource("/files/7/download", {
				signal: controller.signal,
			}),
		).rejects.toMatchObject({ name: "AbortError" });
		expect(mockState.fetch).toHaveBeenCalledTimes(1);
	});

	it("propagates auth failures when refresh succeeds but access is still rejected", async () => {
		mockState.fetch
			.mockResolvedValueOnce(mockProbeResponse(401))
			.mockResolvedValueOnce(mockProbeResponse(401));
		mockState.refreshToken.mockResolvedValueOnce(undefined);
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await expect(
			prepareAuthenticatedResource("/files/7/download"),
		).rejects.toMatchObject({ status: 401 });
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		expect(mockState.fetch).toHaveBeenCalledTimes(2);
		expect(mockState.loggerError).not.toHaveBeenCalled();
	});

	it("propagates auth failures when refresh itself cannot restore access", async () => {
		const events: string[] = [];
		mockState.fetch.mockImplementationOnce(async () => {
			events.push("probe:401");
			return mockProbeResponse(401, events);
		});
		mockState.refreshToken.mockImplementationOnce(async () => {
			events.push("refresh");
			throw { status: 401 };
		});
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await expect(
			prepareAuthenticatedResource("/files/7/download"),
		).rejects.toMatchObject({ status: 401 });
		expect(events).toEqual(["probe:401", "cancel:401", "refresh"]);
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		expect(mockState.fetch).toHaveBeenCalledTimes(1);
		expect(mockState.loggerError).not.toHaveBeenCalled();
	});

	it("reuses the in-flight refresh probe for concurrent callers", async () => {
		let resolveRefresh: (() => void) | null = null;
		mockState.fetch
			.mockResolvedValueOnce(mockProbeResponse(401))
			.mockResolvedValueOnce(mockProbeResponse(206));
		mockState.refreshToken.mockImplementationOnce(
			() =>
				new Promise<void>((resolve) => {
					resolveRefresh = () => {
						mockState.expiresAt += 900_000;
						resolve();
					};
				}),
		);
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		const firstPrepare = prepareAuthenticatedResource("/files/7/download");
		const secondPrepare = prepareAuthenticatedResource("/files/7/download");
		await vi.waitFor(() => {
			expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		});
		resolveRefresh?.();
		await Promise.all([firstPrepare, secondPrepare]);
		await prepareAuthenticatedResource("/files/7/download");

		expect(mockState.ensureFreshSession).toHaveBeenCalledTimes(3);
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		expect(mockState.fetch).toHaveBeenCalledTimes(2);
	});

	it("propagates non-auth probe failures", async () => {
		const error = new Error("cors probe failed");
		mockState.fetch.mockRejectedValue(error);
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await expect(
			prepareAuthenticatedResource("/files/7/download"),
		).rejects.toBe(error);
		expect(mockState.loggerError).toHaveBeenCalledWith(
			"authenticated resource probe failed",
			"/files/7/download",
			error,
		);
	});

	it("reuses a recent probe result for the same path and session", async () => {
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await prepareAuthenticatedResource("/files/7/download");
		await prepareAuthenticatedResource("/files/7/download");

		expect(mockState.ensureFreshSession).toHaveBeenCalledTimes(2);
		expect(mockState.fetch).toHaveBeenCalledTimes(1);
	});

	it("rejects a 200 probe without consuming the full response body", async () => {
		mockProbeStatus(200);
		const { prepareAuthenticatedResource } = await import(
			"@/lib/authenticatedResource"
		);

		await expect(
			prepareAuthenticatedResource("/files/7/download"),
		).rejects.toMatchObject({ status: 200 });
		expect(mockState.cancelBody).toHaveBeenCalledTimes(1);
	});
});

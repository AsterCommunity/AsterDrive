import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	userRouteLoad: vi.fn().mockResolvedValue(undefined),
	adminRouteLoad: vi.fn().mockResolvedValue(undefined),
	loginSuccessPathLoad: vi.fn().mockResolvedValue(undefined),
	userFeatureLoad: vi.fn().mockResolvedValue(undefined),
	previewLoad: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@/lib/pwaWarmupLoaders", () => ({
	userRouteWarmupLoaders: [
		{
			key: "route:user",
			label: "UserRoute",
			load: mockState.userRouteLoad,
		},
	],
	adminRouteWarmupLoaders: [
		{
			key: "route:admin",
			label: "AdminRoute",
			load: mockState.adminRouteLoad,
		},
	],
	loginSuccessPathWarmupLoaders: [
		{
			key: "route:login-success",
			label: "LoginSuccessRoute",
			load: mockState.loginSuccessPathLoad,
		},
	],
	userFeatureWarmupLoaders: [
		{
			key: "feature:file-preview",
			label: "FilePreviewFeature",
			load: mockState.userFeatureLoad,
		},
	],
	filePreviewWarmupLoaders: [
		{
			key: "preview:file",
			label: "FilePreview",
			load: mockState.previewLoad,
		},
	],
}));

async function loadModule() {
	vi.resetModules();
	return await import("@/lib/pwaWarmup");
}

describe("pwaWarmup", () => {
	beforeEach(() => {
		mockState.userRouteLoad.mockClear();
		mockState.adminRouteLoad.mockClear();
		mockState.loginSuccessPathLoad.mockClear();
		mockState.userFeatureLoad.mockClear();
		mockState.previewLoad.mockClear();
		vi.useFakeTimers();
	});

	it("warms the login success path once", async () => {
		const { warmupLoginSuccessPath } = await loadModule();

		warmupLoginSuccessPath();
		warmupLoginSuccessPath();
		await vi.advanceTimersByTimeAsync(4_000);

		expect(mockState.loginSuccessPathLoad).toHaveBeenCalledTimes(1);
		expect(mockState.userRouteLoad).not.toHaveBeenCalled();
		expect(mockState.adminRouteLoad).not.toHaveBeenCalled();
	});

	it("skips the user route queue when no user route warmup is configured", async () => {
		const { warmupRouteChunks } = await loadModule();

		warmupRouteChunks("user");
		await vi.advanceTimersByTimeAsync(5_000);

		expect(mockState.userRouteLoad).not.toHaveBeenCalled();
		expect(mockState.userFeatureLoad).not.toHaveBeenCalled();
		expect(mockState.previewLoad).not.toHaveBeenCalled();
		expect(mockState.adminRouteLoad).not.toHaveBeenCalled();
	});

	it("skips duplicate warmups for the same role", async () => {
		const { warmupRouteChunks } = await loadModule();

		warmupRouteChunks("user");
		warmupRouteChunks("user");
		await vi.advanceTimersByTimeAsync(5_000);

		expect(mockState.userRouteLoad).not.toHaveBeenCalled();
		expect(mockState.userFeatureLoad).not.toHaveBeenCalled();
		expect(mockState.previewLoad).not.toHaveBeenCalled();
	});

	it("warms the admin queue when admin access is warmed first", async () => {
		const { warmupRouteChunks } = await loadModule();

		warmupRouteChunks("admin");
		await vi.advanceTimersByTimeAsync(6_000);

		expect(mockState.userRouteLoad).not.toHaveBeenCalled();
		expect(mockState.adminRouteLoad).toHaveBeenCalledTimes(1);
		expect(mockState.userFeatureLoad).not.toHaveBeenCalled();
		expect(mockState.previewLoad).not.toHaveBeenCalled();
	});

	it("skips later user warmups after the admin queue has already run", async () => {
		const { warmupRouteChunks } = await loadModule();

		warmupRouteChunks("admin");
		await vi.advanceTimersByTimeAsync(6_000);
		warmupRouteChunks("user");
		await vi.advanceTimersByTimeAsync(6_000);

		expect(mockState.userRouteLoad).not.toHaveBeenCalled();
		expect(mockState.adminRouteLoad).toHaveBeenCalledTimes(1);
		expect(mockState.userFeatureLoad).not.toHaveBeenCalled();
		expect(mockState.previewLoad).not.toHaveBeenCalled();
	});

	it("warms preview engines separately and skips duplicates", async () => {
		const { warmupPreviewEngines } = await loadModule();

		warmupPreviewEngines();
		warmupPreviewEngines();
		await vi.advanceTimersByTimeAsync(5_000);

		expect(mockState.userFeatureLoad).toHaveBeenCalledTimes(1);
		expect(mockState.previewLoad).toHaveBeenCalledTimes(1);
		expect(mockState.userRouteLoad).not.toHaveBeenCalled();
		expect(mockState.adminRouteLoad).not.toHaveBeenCalled();
	});
});

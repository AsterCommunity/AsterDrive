import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAdminRemoteNodeDetailController } from "@/pages/admin/useAdminRemoteNodeDetailController";

const mocks = vi.hoisted(() => ({
	addResourceBundle: vi.fn(),
	get: vi.fn(),
	listStorageTargetConnectors: vi.fn(),
	listStorageTargets: vi.fn(),
	navigate: vi.fn(),
}));

const testI18n = vi.hoisted(() => ({
	addResourceBundle: (...args: unknown[]) => mocks.addResourceBundle(...args),
	language: "zh-CN",
	resolvedLanguage: "zh-CN",
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		i18n: testI18n,
		// A newly bound function on every render reproduces the identity change
		// emitted by react-i18next after a resource bundle is installed.
		t: (key: string) => key,
	}),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mocks.navigate,
}));

vi.mock("@/hooks/useApiError", () => ({
	getApiErrorMessage: (error: Error) => error.message,
	handleApiError: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminRemoteNodeService: {
		get: (...args: unknown[]) => mocks.get(...args),
		listStorageTargetConnectors: (...args: unknown[]) =>
			mocks.listStorageTargetConnectors(...args),
		listStorageTargets: (...args: unknown[]) =>
			mocks.listStorageTargets(...args),
	},
}));

describe("useAdminRemoteNodeDetailController", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.get.mockResolvedValue({
			base_url: "https://follower.example.com",
			capabilities: null,
			created_at: "2026-09-09T00:00:00Z",
			enrollment_status: "completed",
			id: 21,
			is_enabled: true,
			last_probe_at: null,
			last_probe_error: "",
			name: "Follower",
			transport_mode: "direct",
			updated_at: "2026-09-09T00:00:00Z",
		});
		mocks.listStorageTargets.mockResolvedValue([]);
		mocks.listStorageTargetConnectors.mockResolvedValue({
			descriptors: [],
			localizations: {
				requested_locale: "zh-CN",
				resources: [
					{
						connector_id: "plugin.follower-only",
						messages: { driver_type_follower: "Follower 专用存储" },
						namespace: "plugin.follower-only",
						requested_locale: "zh-CN",
						resolved_locale: "zh",
						revision: "1",
					},
				],
			},
		});
	});

	it("does not reload follower catalogs when localization installation rebinds t", async () => {
		renderHook(() => useAdminRemoteNodeDetailController(21, "storage-targets"));

		await waitFor(() => {
			expect(mocks.addResourceBundle).toHaveBeenCalledWith(
				"zh-CN",
				"plugin.follower-only",
				{ driver_type_follower: "Follower 专用存储" },
				true,
				true,
			);
		});
		await waitFor(() => {
			expect(mocks.listStorageTargetConnectors).toHaveBeenCalledTimes(1);
			expect(mocks.listStorageTargets).toHaveBeenCalledTimes(1);
		});
		expect(mocks.listStorageTargetConnectors).toHaveBeenCalledWith(21, "zh-CN");
	});
});

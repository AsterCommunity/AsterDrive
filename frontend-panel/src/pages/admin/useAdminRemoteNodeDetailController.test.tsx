import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAdminRemoteNodeDetailController } from "@/pages/admin/useAdminRemoteNodeDetailController";

const mocks = vi.hoisted(() => ({
	addResourceBundle: vi.fn(),
	createEnrollmentCommand: vi.fn(),
	createStorageTarget: vi.fn(),
	deleteStorageTarget: vi.fn(),
	get: vi.fn(),
	listStorageTargetConnectors: vi.fn(),
	listStorageTargets: vi.fn(),
	navigate: vi.fn(),
	testConnection: vi.fn(),
	update: vi.fn(),
	updateStorageTarget: vi.fn(),
	copy: vi.fn(),
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
		createEnrollmentCommand: (...args: unknown[]) =>
			mocks.createEnrollmentCommand(...args),
		createStorageTarget: (...args: unknown[]) =>
			mocks.createStorageTarget(...args),
		deleteStorageTarget: (...args: unknown[]) =>
			mocks.deleteStorageTarget(...args),
		get: (...args: unknown[]) => mocks.get(...args),
		listStorageTargetConnectors: (...args: unknown[]) =>
			mocks.listStorageTargetConnectors(...args),
		listStorageTargets: (...args: unknown[]) =>
			mocks.listStorageTargets(...args),
		testConnection: (...args: unknown[]) => mocks.testConnection(...args),
		update: (...args: unknown[]) => mocks.update(...args),
		updateStorageTarget: (...args: unknown[]) =>
			mocks.updateStorageTarget(...args),
	},
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: (...args: unknown[]) => mocks.copy(...args),
}));

vi.mock("sonner", () => ({
	toast: { success: vi.fn(), error: vi.fn() },
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
		mocks.createEnrollmentCommand.mockResolvedValue({
			command: "asterdrive follower enroll ...",
			expires_at: "2026-09-09T01:00:00Z",
		});
		mocks.testConnection.mockResolvedValue({
			base_url: "https://follower.example.com",
			capabilities: null,
			created_at: "2026-09-09T00:00:00Z",
			enrollment_status: "completed",
			id: 21,
			is_enabled: true,
			last_probe_at: "2026-09-09T00:01:00Z",
			last_probe_error: "",
			name: "Follower",
			transport_mode: "direct",
			updated_at: "2026-09-09T00:01:00Z",
		});
		mocks.update.mockResolvedValue({
			base_url: "https://follower.example.com",
			capabilities: null,
			created_at: "2026-09-09T00:00:00Z",
			enrollment_status: "completed",
			id: 21,
			is_enabled: false,
			last_probe_at: null,
			last_probe_error: "",
			name: "Updated follower",
			transport_mode: "direct",
			updated_at: "2026-09-09T00:02:00Z",
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

	it("loads a completed node on the overview tab without loading target catalogs", async () => {
		const { result } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "overview"),
		);

		await waitFor(() => expect(result.current.loading).toBe(false));
		expect(result.current.node?.name).toBe("Follower");
		expect(result.current.form?.base_url).toBe("https://follower.example.com");
		expect(mocks.listStorageTargets).not.toHaveBeenCalled();
		expect(mocks.listStorageTargetConnectors).not.toHaveBeenCalled();
	});

	it("surfaces node and catalog loading failures", async () => {
		mocks.get.mockRejectedValueOnce(new Error("node unavailable"));
		const failedNode = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "overview"),
		);
		await waitFor(() => expect(failedNode.result.current.loading).toBe(false));
		expect(failedNode.result.current.node).toBeNull();

		mocks.get.mockResolvedValueOnce({
			base_url: "https://follower.example.com",
			capabilities: null,
			created_at: "",
			enrollment_status: "completed",
			id: 21,
			is_enabled: true,
			last_probe_at: null,
			last_probe_error: "",
			name: "Follower",
			transport_mode: "direct",
			updated_at: "",
		});
		mocks.listStorageTargets.mockRejectedValueOnce(new Error("targets failed"));
		mocks.listStorageTargetConnectors.mockRejectedValueOnce(
			new Error("descriptors failed"),
		);
		const catalogs = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "storage-targets"),
		);
		await waitFor(() =>
			expect(catalogs.result.current.remoteStorageTargetsError).toBe(
				"targets failed",
			),
		);
		expect(
			catalogs.result.current.remoteStorageTargetConnectorDescriptorsError,
		).toBe("descriptors failed");
	});

	it("guards target loading for incomplete enrollment and missing direct base URL", async () => {
		mocks.get.mockResolvedValueOnce({
			base_url: "",
			capabilities: null,
			created_at: "",
			enrollment_status: "completed",
			id: 21,
			is_enabled: true,
			last_probe_at: null,
			last_probe_error: "",
			name: "Follower",
			transport_mode: "direct",
			updated_at: "",
		});
		let pageTab: "overview" | "storage-targets" = "storage-targets";
		const { result, rerender } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, pageTab),
		);
		await waitFor(() => expect(result.current.loading).toBe(false));
		expect(result.current.remoteStorageTargetsError).toBe(
			"remote_node_ingress_profiles_base_url_required",
		);
		expect(mocks.listStorageTargets).not.toHaveBeenCalled();
		pageTab = "overview";
		rerender();

		mocks.get.mockResolvedValueOnce({
			base_url: "https://pending.example.com",
			capabilities: null,
			created_at: "",
			enrollment_status: "pending",
			id: 21,
			is_enabled: true,
			last_probe_at: null,
			last_probe_error: "",
			name: "Pending",
			transport_mode: "direct",
			updated_at: "",
		});
		const pending = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "storage-targets"),
		);
		await waitFor(() => expect(pending.result.current.loading).toBe(false));
		expect(pending.result.current.remoteStorageTargets).toEqual([]);
	});

	it("supports copy, enrollment, save, connection test, target mutations, and navigation", async () => {
		const { result } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "storage-targets"),
		);
		await waitFor(() => expect(result.current.loading).toBe(false));

		await act(async () => {
			await result.current.copyToClipboard("secret command");
			await result.current.generateEnrollmentCommand();
			await result.current.runConnectionTest();
			await result.current.submit();
			await result.current.createRemoteStorageTarget({
				target_key: "archive",
				connector_id: "local",
				config: {},
				credentials: {},
			});
			await result.current.updateRemoteStorageTarget("archive", {
				connector_id: "local",
				config: {},
				credentials: {},
			});
			await result.current.deleteRemoteStorageTarget({
				target_key: "archive",
				connector_id: "local",
				config: {},
				created_at: "",
				updated_at: "",
			});
			result.current.setField("name", "Renamed");
			result.current.navigateBack();
		});

		expect(mocks.copy).toHaveBeenCalledWith("secret command");
		expect(mocks.createEnrollmentCommand).not.toHaveBeenCalled();
		expect(mocks.testConnection).toHaveBeenCalledWith(21);
		expect(mocks.update).toHaveBeenCalledWith(
			21,
			expect.objectContaining({
				name: "Follower",
				base_url: "https://follower.example.com",
			}),
		);
		expect(mocks.createStorageTarget).toHaveBeenCalledWith(
			21,
			expect.anything(),
		);
		expect(mocks.updateStorageTarget).toHaveBeenCalledWith(
			21,
			"archive",
			expect.anything(),
		);
		expect(mocks.deleteStorageTarget).toHaveBeenCalledWith(21, "archive");
		expect(mocks.navigate).toHaveBeenCalledWith("/admin/remote-nodes", {
			viewTransition: false,
		});
	});

	it("handles copy and save failures without leaving loading state stuck", async () => {
		mocks.copy.mockRejectedValueOnce(new Error("clipboard unavailable"));
		mocks.update.mockRejectedValueOnce(new Error("save failed"));
		const { result } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "overview"),
		);
		await waitFor(() => expect(result.current.loading).toBe(false));
		await act(async () => {
			await result.current.copyToClipboard("command");
			await result.current.submit();
		});
		expect(result.current.submitting).toBe(false);
	});

	it("generates enrollment commands for an incomplete node and handles failures", async () => {
		mocks.get.mockResolvedValueOnce({
			base_url: "https://pending.example.com",
			capabilities: null,
			created_at: "",
			enrollment_status: "pending",
			id: 21,
			is_enabled: true,
			last_probe_at: null,
			last_probe_error: "",
			name: "Pending",
			transport_mode: "direct",
			updated_at: "",
		});
		mocks.createEnrollmentCommand.mockRejectedValueOnce(
			new Error("command failed"),
		);
		const { result } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "overview"),
		);
		await waitFor(() => expect(result.current.loading).toBe(false));
		await act(async () => {
			await result.current.generateEnrollmentCommand();
		});
		expect(result.current.enrollmentCommandError).toBe("command failed");
		expect(result.current.enrollmentCommandLoading).toBe(false);
	});

	it("stores a generated enrollment command and rejects dirty connection tests", async () => {
		mocks.get.mockResolvedValueOnce({
			base_url: "https://pending.example.com",
			capabilities: null,
			created_at: "",
			enrollment_status: "pending",
			id: 21,
			is_enabled: true,
			last_probe_at: null,
			last_probe_error: "",
			name: "Pending",
			transport_mode: "direct",
			updated_at: "",
		});
		mocks.createEnrollmentCommand.mockResolvedValueOnce({
			command: "asterdrive follower enroll --token abc",
			expires_at: "2026-09-09T01:00:00Z",
		});
		const { result } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "overview"),
		);
		await waitFor(() => expect(result.current.loading).toBe(false));
		await act(async () => {
			await result.current.generateEnrollmentCommand();
		});
		expect(result.current.enrollmentCommand?.command).toContain("enroll");

		await act(async () => {
			result.current.setField("base_url", "https://changed.example.com");
		});
		expect(await result.current.runConnectionTest()).toBe(false);
		expect(mocks.testConnection).not.toHaveBeenCalled();
	});

	it("does not call target APIs for an incomplete enrollment", async () => {
		mocks.get.mockResolvedValueOnce({
			base_url: "https://pending.example.com",
			capabilities: null,
			created_at: "",
			enrollment_status: "pending",
			id: 21,
			is_enabled: true,
			last_probe_at: null,
			last_probe_error: "",
			name: "Pending",
			transport_mode: "direct",
			updated_at: "",
		});
		const { result } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "storage-targets"),
		);
		await waitFor(() => expect(result.current.loading).toBe(false));
		await act(async () => {
			await result.current.createRemoteStorageTarget({
				target_key: "archive",
				connector_id: "local",
				config: {},
				credentials: {},
			});
		});
		expect(mocks.createStorageTarget).toHaveBeenCalledWith(
			21,
			expect.anything(),
		);
		expect(mocks.listStorageTargets).not.toHaveBeenCalled();
	});

	it("routes target mutation failures to the shared error handler", async () => {
		mocks.createStorageTarget.mockRejectedValueOnce(new Error("create failed"));
		mocks.updateStorageTarget.mockRejectedValueOnce(new Error("update failed"));
		mocks.deleteStorageTarget.mockRejectedValueOnce(new Error("delete failed"));
		const { result } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "storage-targets"),
		);
		await waitFor(() => expect(result.current.loading).toBe(false));
		const payload = {
			connector_id: "local",
			config: {},
			credentials: {},
		};
		await expect(
			result.current.createRemoteStorageTarget({ target_key: "a", ...payload }),
		).rejects.toThrow("create failed");
		await expect(
			result.current.updateRemoteStorageTarget("a", payload),
		).rejects.toThrow("update failed");
		await expect(
			result.current.deleteRemoteStorageTarget({
				target_key: "a",
				connector_id: "local",
				config: {},
				created_at: "",
				updated_at: "",
			}),
		).rejects.toThrow("delete failed");
	});

	it("refreshes after connection-test failure and tolerates a failed refresh", async () => {
		mocks.testConnection.mockRejectedValueOnce(new Error("probe failed"));
		mocks.get.mockResolvedValueOnce({
			base_url: "https://follower.example.com",
			capabilities: null,
			created_at: "",
			enrollment_status: "completed",
			id: 21,
			is_enabled: true,
			last_probe_at: null,
			last_probe_error: "probe failed",
			name: "Follower",
			transport_mode: "direct",
			updated_at: "",
		});
		const { result } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, "overview"),
		);
		await waitFor(() => expect(result.current.loading).toBe(false));
		await act(async () =>
			expect(await result.current.runConnectionTest()).toBe(false),
		);
		expect(mocks.get).toHaveBeenCalledTimes(2);

		mocks.testConnection.mockRejectedValueOnce(new Error("probe failed again"));
		mocks.get.mockRejectedValueOnce(new Error("refresh failed"));
		await act(async () =>
			expect(await result.current.runConnectionTest()).toBe(false),
		);
	});

	it("cancels descriptor loading when leaving the target tab", async () => {
		mocks.get.mockResolvedValueOnce({
			base_url: "",
			capabilities: null,
			created_at: "",
			enrollment_status: "completed",
			id: 21,
			is_enabled: true,
			last_probe_at: null,
			last_probe_error: "",
			name: "Reverse follower",
			transport_mode: "reverse_tunnel",
			updated_at: "",
		});
		let pageTab: "overview" | "storage-targets" = "storage-targets";
		const { rerender } = renderHook(() =>
			useAdminRemoteNodeDetailController(21, pageTab),
		);
		await waitFor(() => expect(mocks.listStorageTargets).toHaveBeenCalled());
		pageTab = "overview";
		rerender();
	});
});

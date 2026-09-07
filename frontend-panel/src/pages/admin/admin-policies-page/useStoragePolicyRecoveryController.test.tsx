import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { adminPolicyService } from "@/services/adminService";
import type {
	StoragePolicy,
	StoragePolicyForcedPurgePreview,
	StoragePolicyRecoveryProbe,
} from "@/types/api";
import { useStoragePolicyRecoveryController } from "./useStoragePolicyRecoveryController";

const mockNavigate = vi.fn();
const mockToastSuccess = vi.fn();
const mockHandleApiError = vi.fn();

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockNavigate,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: Record<string, unknown>) =>
			values?.id == null ? key : `${key}:${String(values.id)}`,
	}),
}));

vi.mock("sonner", () => ({
	toast: { success: (...args: unknown[]) => mockToastSuccess(...args) },
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockHandleApiError(...args),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		createForcedPurge: vi.fn(),
		createMigration: vi.fn(),
		listAll: vi.fn(),
		previewForcedPurge: vi.fn(),
		probeRecovery: vi.fn(),
	},
}));

function policy(id: number, name = `Policy ${id}`): StoragePolicy {
	return {
		allowed_types: [],
		behavior: {},
		chunk_size: 1024,
		connector_config: {
			connector_id: "asterdrive.storage.local",
			format_version: 1,
			schema_version: 1,
			values: {},
		},
		connector_id: "asterdrive.storage.local",
		created_at: "2026-09-07T00:00:00Z",
		id,
		is_default: false,
		max_file_size: 0,
		name,
		updated_at: "2026-09-07T00:00:00Z",
	};
}

function probe(
	policyId: number,
	overrides: Partial<StoragePolicyRecoveryProbe> = {},
): StoragePolicyRecoveryProbe {
	return {
		can_start_recovery: true,
		plan_hash: `probe-${policyId}`,
		policy_id: policyId,
		policy_updated_at: "2026-09-07T00:00:00Z",
		samples: [],
		status: "recoverable",
		stored_blob_count: 1,
		stored_total_bytes: 16,
		virtual_empty_blob_count: 0,
		...overrides,
	};
}

function preview(policyId: number): StoragePolicyForcedPurgePreview {
	return {
		affected_logical_bytes: 16,
		affected_revision_count: 1,
		blob_count: 1,
		blob_total_bytes: 16,
		can_start: true,
		confirmation_phrase: `DELETE Policy ${policyId}`,
		direct_share_count: 0,
		file_count: 1,
		impact_digest: `impact-${policyId}`,
		placement_target_count: 0,
		policy_id: policyId,
		policy_name: `Policy ${policyId}`,
		policy_updated_at: "2026-09-07T00:00:00Z",
		trash_file_count: 0,
		upload_session_count: 0,
	};
}

/** Creates a controllable promise for stale-response and loading-state tests. */
function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason: unknown) => void;
	const promise = new Promise<T>((resolvePromise, rejectPromise) => {
		resolve = resolvePromise;
		reject = rejectPromise;
	});
	return { promise, reject, resolve };
}

describe("useStoragePolicyRecoveryController", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.mocked(adminPolicyService.listAll).mockResolvedValue([
			policy(1),
			policy(2),
		]);
		vi.mocked(adminPolicyService.probeRecovery).mockResolvedValue(probe(1));
		vi.mocked(adminPolicyService.previewForcedPurge).mockResolvedValue(
			preview(1),
		);
		vi.mocked(adminPolicyService.createMigration).mockResolvedValue({
			id: 41,
		} as never);
		vi.mocked(adminPolicyService.createForcedPurge).mockResolvedValue({
			id: 42,
		} as never);
	});

	it("loads probe and target policies when opening", async () => {
		const { result } = renderHook(() => useStoragePolicyRecoveryController());

		await act(async () => {
			await result.current.openForPolicy(policy(1));
		});

		expect(result.current.open).toBe(true);
		expect(result.current.probe).toEqual(probe(1));
		expect(result.current.policies.map((item) => item.id)).toEqual([1, 2]);
		expect(result.current.targetPolicyId).toBe("2");
		expect(result.current.loading).toBe(false);
	});

	it("preserves a successful probe when the target policy list fails", async () => {
		const listError = new Error("list failed");
		vi.mocked(adminPolicyService.listAll).mockRejectedValueOnce(listError);
		const { result } = renderHook(() => useStoragePolicyRecoveryController());

		await act(async () => {
			await result.current.openForPolicy(policy(1));
		});

		expect(result.current.probe).toEqual(probe(1));
		expect(result.current.policies).toEqual([]);
		expect(result.current.targetPolicyId).toBe("");
		expect(mockHandleApiError).toHaveBeenCalledWith(listError);
	});

	it("preserves target policies when the probe fails", async () => {
		const probeError = new Error("probe failed");
		vi.mocked(adminPolicyService.probeRecovery).mockRejectedValueOnce(
			probeError,
		);
		const { result } = renderHook(() => useStoragePolicyRecoveryController());

		await act(async () => {
			await result.current.openForPolicy(policy(1));
		});

		expect(result.current.probe).toBeNull();
		expect(result.current.policies.map((item) => item.id)).toEqual([1, 2]);
		expect(result.current.targetPolicyId).toBe("2");
		expect(mockHandleApiError).toHaveBeenCalledWith(probeError);
	});

	it("clears stale evidence and reloads both dependencies on retry", async () => {
		const { result } = renderHook(() => useStoragePolicyRecoveryController());
		await act(async () => {
			await result.current.openForPolicy(policy(1));
		});

		const listRequest = deferred<StoragePolicy[]>();
		const probeRequest = deferred<StoragePolicyRecoveryProbe>();
		vi.mocked(adminPolicyService.listAll).mockReturnValueOnce(
			listRequest.promise,
		);
		vi.mocked(adminPolicyService.probeRecovery).mockReturnValueOnce(
			probeRequest.promise,
		);
		let retryPromise: Promise<void> | undefined;
		act(() => {
			retryPromise = result.current.retryProbe();
		});

		expect(result.current.loading).toBe(true);
		expect(result.current.probe).toBeNull();
		expect(result.current.policies).toEqual([]);
		expect(result.current.targetPolicyId).toBe("");

		await act(async () => {
			listRequest.resolve([policy(1), policy(3)]);
			probeRequest.resolve(probe(1, { plan_hash: "refreshed" }));
			await retryPromise;
		});

		expect(adminPolicyService.listAll).toHaveBeenCalledTimes(2);
		expect(adminPolicyService.probeRecovery).toHaveBeenCalledTimes(2);
		expect(result.current.probe?.plan_hash).toBe("refreshed");
		expect(result.current.targetPolicyId).toBe("3");
	});

	it("ignores successful responses from an older policy workflow", async () => {
		const oldList = deferred<StoragePolicy[]>();
		const oldProbe = deferred<StoragePolicyRecoveryProbe>();
		vi.mocked(adminPolicyService.listAll)
			.mockReturnValueOnce(oldList.promise)
			.mockResolvedValueOnce([policy(2), policy(3)]);
		vi.mocked(adminPolicyService.probeRecovery)
			.mockReturnValueOnce(oldProbe.promise)
			.mockResolvedValueOnce(probe(2));
		const { result } = renderHook(() => useStoragePolicyRecoveryController());

		act(() => {
			void result.current.openForPolicy(policy(1));
		});
		await act(async () => {
			await result.current.openForPolicy(policy(2));
		});
		await act(async () => {
			oldList.resolve([policy(1), policy(4)]);
			oldProbe.resolve(probe(1));
			await Promise.all([oldList.promise, oldProbe.promise]);
		});

		expect(result.current.policy?.id).toBe(2);
		expect(result.current.probe?.policy_id).toBe(2);
		expect(result.current.policies.map((item) => item.id)).toEqual([2, 3]);
		expect(result.current.targetPolicyId).toBe("3");
	});

	it("ignores errors from a closed workflow", async () => {
		const oldList = deferred<StoragePolicy[]>();
		const oldProbe = deferred<StoragePolicyRecoveryProbe>();
		vi.mocked(adminPolicyService.listAll).mockReturnValueOnce(oldList.promise);
		vi.mocked(adminPolicyService.probeRecovery).mockReturnValueOnce(
			oldProbe.promise,
		);
		const { result } = renderHook(() => useStoragePolicyRecoveryController());

		act(() => {
			void result.current.openForPolicy(policy(1));
			result.current.setOpen(false);
		});
		await act(async () => {
			oldList.reject(new Error("stale list failure"));
			oldProbe.reject(new Error("stale probe failure"));
			await Promise.allSettled([oldList.promise, oldProbe.promise]);
		});

		expect(result.current.open).toBe(false);
		expect(mockHandleApiError).not.toHaveBeenCalled();
	});

	it("does not apply a purge preview after switching policies", async () => {
		const previewRequest = deferred<StoragePolicyForcedPurgePreview>();
		const { result } = renderHook(() => useStoragePolicyRecoveryController());
		await act(async () => {
			await result.current.openForPolicy(policy(1));
		});
		vi.mocked(adminPolicyService.previewForcedPurge).mockReturnValueOnce(
			previewRequest.promise,
		);
		act(() => {
			void result.current.previewForcedPurge();
		});
		vi.mocked(adminPolicyService.probeRecovery).mockResolvedValueOnce(probe(2));
		await act(async () => {
			await result.current.openForPolicy(policy(2));
		});
		await act(async () => {
			previewRequest.resolve(preview(1));
			await previewRequest.promise;
		});

		expect(result.current.policy?.id).toBe(2);
		expect(result.current.purgePreview).toBeNull();
	});

	it("creates a recover-available migration from current probe evidence", async () => {
		const { result } = renderHook(() => useStoragePolicyRecoveryController());
		await act(async () => {
			await result.current.openForPolicy(policy(1));
		});
		await act(async () => {
			await result.current.startRecovery();
		});

		expect(adminPolicyService.createMigration).toHaveBeenCalledWith({
			mode: "recover_available",
			recovery_plan_hash: "probe-1",
			source_policy_id: 1,
			target_policy_id: 2,
		});
		expect(result.current.open).toBe(false);
		expect(mockToastSuccess).toHaveBeenCalledWith(
			"policy_recovery_task_created:41",
		);
		expect(mockNavigate).toHaveBeenCalledWith(
			"/admin/tasks?kind=storage_policy_migration",
			{ viewTransition: false },
		);
	});

	it("previews and creates a confirmed forced purge with an optional reason", async () => {
		const { result } = renderHook(() => useStoragePolicyRecoveryController());
		await act(async () => {
			await result.current.openForPolicy(policy(1));
		});
		await act(async () => {
			await result.current.previewForcedPurge();
		});
		act(() => {
			result.current.setConfirmation("DELETE Policy 1");
			result.current.setReason("storage endpoint retired");
		});
		await act(async () => {
			await result.current.startForcedPurge();
		});

		expect(adminPolicyService.createForcedPurge).toHaveBeenCalledWith(1, {
			confirmation: "DELETE Policy 1",
			impact_digest: "impact-1",
			reason: "storage endpoint retired",
		});
		expect(result.current.open).toBe(false);
		expect(mockToastSuccess).toHaveBeenCalledWith(
			"policy_forced_purge_task_created:42",
		);
		expect(mockNavigate).toHaveBeenCalledWith(
			"/admin/tasks?kind=storage_policy_forced_purge",
			{ viewTransition: false },
		);
	});

	it("rejects an invalid recovery target without creating a task", async () => {
		const { result } = renderHook(() => useStoragePolicyRecoveryController());
		await act(async () => {
			await result.current.openForPolicy(policy(1));
		});
		act(() => result.current.setTargetPolicyId("1"));
		await act(async () => result.current.startRecovery());
		expect(adminPolicyService.createMigration).not.toHaveBeenCalled();
	});

	it("reports current recovery, preview, and forced-purge request errors", async () => {
		const recoveryError = new Error("migration failed");
		const previewError = new Error("preview failed");
		const purgeError = new Error("purge failed");
		vi.mocked(adminPolicyService.createMigration).mockRejectedValueOnce(
			recoveryError,
		);
		vi.mocked(adminPolicyService.previewForcedPurge)
			.mockRejectedValueOnce(previewError)
			.mockResolvedValueOnce(preview(1));
		vi.mocked(adminPolicyService.createForcedPurge).mockRejectedValueOnce(
			purgeError,
		);
		const { result } = renderHook(() => useStoragePolicyRecoveryController());
		await act(async () => {
			await result.current.openForPolicy(policy(1));
		});

		await act(async () => result.current.startRecovery());
		await act(async () => result.current.previewForcedPurge());
		await act(async () => result.current.previewForcedPurge());
		await act(async () => result.current.startForcedPurge());

		expect(mockHandleApiError).toHaveBeenNthCalledWith(1, recoveryError);
		expect(mockHandleApiError).toHaveBeenNthCalledWith(2, previewError);
		expect(mockHandleApiError).toHaveBeenNthCalledWith(3, purgeError);
		expect(result.current.submitting).toBe(false);
		expect(result.current.loading).toBe(false);
	});

	it("accepts an explicit open state before closing and invalidating requests", () => {
		const { result } = renderHook(() => useStoragePolicyRecoveryController());
		act(() => result.current.setOpen(true));
		expect(result.current.open).toBe(true);
		act(() => result.current.setOpen(false));
		expect(result.current.open).toBe(false);
	});
});

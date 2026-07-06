import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useStoragePolicyMigrationController } from "@/pages/admin/admin-policies-page/useStoragePolicyMigrationController";
import { adminPolicyService } from "@/services/adminService";
import type { StoragePolicy, StoragePolicyMigrationDryRun } from "@/types/api";

const mockNavigate = vi.fn();
const mockToastError = vi.fn();
const mockToastSuccess = vi.fn();
const mockHandleApiError = vi.fn();

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockNavigate,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: Record<string, string | number>) =>
			values?.id != null ? `${key}:${values.id}` : key,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockToastError(...args),
		success: (...args: unknown[]) => mockToastSuccess(...args),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockHandleApiError(...args),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		createMigration: vi.fn(),
		dryRunMigration: vi.fn(),
		listAll: vi.fn(),
	},
}));

function policy(id: number, name = `Policy ${id}`): StoragePolicy {
	return {
		allowed_types: [],
		base_path: "",
		bucket: "",
		chunk_size: 5 * 1024 * 1024,
		created_at: "2026-01-01T00:00:00Z",
		driver_type: "local",
		endpoint: "",
		id,
		is_default: id === 1,
		max_file_size: null,
		name,
		options: {},
		updated_at: "2026-01-01T00:00:00Z",
	} as unknown as StoragePolicy;
}

function dryRun(
	overrides: Partial<StoragePolicyMigrationDryRun> = {},
): StoragePolicyMigrationDryRun {
	return {
		can_start: true,
		content_sha256_blob_count: 0,
		delete_source_after_success_supported: false,
		estimated_copy_blob_count: 2,
		opaque_blob_count: 2,
		opaque_key_conflict_count: 0,
		source_blob_count: 2,
		source_policy_id: 1,
		source_total_bytes: 1024,
		target_capacity: null,
		target_capacity_check: "skipped",
		target_connection_ok: true,
		target_matching_blob_count: 0,
		target_policy_id: 2,
		target_supports_stream_upload: true,
		warnings: [],
		...overrides,
	} as StoragePolicyMigrationDryRun;
}

describe("useStoragePolicyMigrationController", () => {
	beforeEach(() => {
		mockNavigate.mockReset();
		mockToastError.mockReset();
		mockToastSuccess.mockReset();
		mockHandleApiError.mockReset();
		vi.mocked(adminPolicyService.createMigration).mockReset();
		vi.mocked(adminPolicyService.dryRunMigration).mockReset();
		vi.mocked(adminPolicyService.listAll).mockReset();
		vi.mocked(adminPolicyService.listAll).mockResolvedValue([
			policy(1, "Source"),
			policy(2, "Target"),
		]);
		vi.mocked(adminPolicyService.dryRunMigration).mockResolvedValue(dryRun());
		vi.mocked(adminPolicyService.createMigration).mockResolvedValue({
			id: 42,
			kind: "storage_policy_migration",
		});
	});

	it("loads policies and seeds source and target when opening", async () => {
		const { result } = renderHook(() => useStoragePolicyMigrationController());

		await act(async () => {
			await result.current.openDialog();
		});

		expect(result.current.open).toBe(true);
		expect(result.current.policies.map((item) => item.name)).toEqual([
			"Source",
			"Target",
		]);
		expect(result.current.sourcePolicyId).toBe("1");
		expect(result.current.targetPolicyId).toBe("2");
		expect(result.current.dryRun).toBeNull();
	});

	it("clears invalid same-policy selections", async () => {
		const { result } = renderHook(() => useStoragePolicyMigrationController());
		await act(async () => {
			await result.current.openDialog();
		});

		act(() => {
			result.current.handleSourcePolicyChange("2");
		});

		expect(result.current.sourcePolicyId).toBe("2");
		expect(result.current.targetPolicyId).toBe("");
		expect(result.current.dryRun).toBeNull();

		act(() => {
			result.current.handleTargetPolicyChange("2");
		});

		expect(mockToastError).toHaveBeenCalledWith(
			"policy_migration_same_policy_error",
		);
	});

	it("dry-runs and creates a migration task", async () => {
		const { result } = renderHook(() => useStoragePolicyMigrationController());
		await act(async () => {
			await result.current.openDialog();
		});

		await act(async () => {
			await result.current.dryRunMigration();
		});

		expect(adminPolicyService.dryRunMigration).toHaveBeenCalledWith({
			delete_source_after_success: false,
			source_policy_id: 1,
			target_policy_id: 2,
		});
		expect(result.current.dryRun).toMatchObject({
			can_start: true,
			source_policy_id: 1,
			target_policy_id: 2,
		});

		await act(async () => {
			await result.current.createMigration();
		});

		await waitFor(() => {
			expect(adminPolicyService.createMigration).toHaveBeenCalledWith({
				delete_source_after_success: false,
				source_policy_id: 1,
				target_policy_id: 2,
			});
		});
		expect(result.current.open).toBe(false);
		expect(mockToastSuccess).toHaveBeenCalledWith(
			"policy_migration_created:42",
		);
		expect(mockNavigate).toHaveBeenCalledWith(
			"/admin/tasks?kind=storage_policy_migration",
			{ viewTransition: false },
		);
	});

	it("does not create a migration without a matching successful dry run", async () => {
		vi.mocked(adminPolicyService.dryRunMigration).mockResolvedValue(
			dryRun({ can_start: false }),
		);
		const { result } = renderHook(() => useStoragePolicyMigrationController());
		await act(async () => {
			await result.current.openDialog();
			await result.current.dryRunMigration();
			await result.current.createMigration();
		});

		expect(adminPolicyService.createMigration).not.toHaveBeenCalled();
	});

	it("reports loading and dry-run errors", async () => {
		const openError = new Error("load failed");
		vi.mocked(adminPolicyService.listAll).mockRejectedValue(openError);
		const { result } = renderHook(() => useStoragePolicyMigrationController());

		await act(async () => {
			await result.current.openDialog();
		});

		expect(result.current.open).toBe(false);
		expect(mockHandleApiError).toHaveBeenCalledWith(openError);
		mockHandleApiError.mockClear();

		vi.mocked(adminPolicyService.listAll).mockResolvedValue([
			policy(1),
			policy(2),
		]);
		const dryRunError = new Error("dry-run failed");
		vi.mocked(adminPolicyService.dryRunMigration).mockRejectedValue(
			dryRunError,
		);
		await act(async () => {
			await result.current.openDialog();
		});
		await act(async () => {
			await result.current.dryRunMigration();
		});

		expect(result.current.dryRun).toBeNull();
		expect(mockHandleApiError).toHaveBeenCalledWith(dryRunError);
	});
});

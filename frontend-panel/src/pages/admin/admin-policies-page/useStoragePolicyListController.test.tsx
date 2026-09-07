import { act, renderHook, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { adminPolicyService } from "@/services/adminService";
import { ApiError } from "@/services/http";
import type { StoragePolicy } from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";
import { useStoragePolicyListController } from "./useStoragePolicyListController";

const mockHandleApiError = vi.fn();

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockHandleApiError(...args),
}));

vi.mock("@/lib/adminPolicyLookup", () => ({
	invalidateAdminPolicyLookup: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		delete: vi.fn(),
		list: vi.fn(),
	},
}));

const sourcePolicy = {
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
	id: 7,
	is_default: false,
	max_file_size: 0,
	name: "Lost storage",
	updated_at: "2026-09-07T00:00:00Z",
} satisfies StoragePolicy;

function wrapper({ children }: { children: React.ReactNode }) {
	return <MemoryRouter>{children}</MemoryRouter>;
}

describe("useStoragePolicyListController", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.mocked(adminPolicyService.list).mockResolvedValue({
			items: [sourcePolicy],
			total: 1,
		});
	});

	it("opens recovery when guarded deletion reports blob references", async () => {
		const blocked = new ApiError(
			ApiErrorCode.PolicyBlobReferencesExist,
			"policy still has blob references",
		);
		vi.mocked(adminPolicyService.delete).mockRejectedValueOnce(blocked);
		const onBlobReferencesBlocked = vi.fn();
		const { result } = renderHook(
			() =>
				useStoragePolicyListController({
					onBlobReferencesBlocked,
				}),
			{ wrapper },
		);
		await waitFor(() =>
			expect(result.current.policies).toEqual([sourcePolicy]),
		);

		act(() => {
			result.current.requestDeleteConfirm(sourcePolicy.id);
		});
		act(() => {
			result.current.deleteDialogProps.onConfirm();
		});

		await waitFor(() => {
			expect(onBlobReferencesBlocked).toHaveBeenCalledWith(sourcePolicy);
		});
		expect(mockHandleApiError).not.toHaveBeenCalled();
		expect(result.current.deletingPolicyId).toBeNull();
	});
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PoliciesTable } from "@/components/admin/admin-policies-page/PoliciesTable";
import { invalidateAdminStorageDriverDescriptors } from "@/lib/adminStorageDriverDescriptors";
import AdminPoliciesPage from "@/pages/admin/AdminPoliciesPage";
import type { StoragePolicy } from "@/types/api";

type TableProps = ComponentProps<typeof PoliciesTable>;

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	list: vi.fn(),
	listStorageDriverDescriptors: vi.fn(),
	navigate: vi.fn(),
	reload: vi.fn(),
	searchParams: new URLSearchParams(),
	setSearchParams: vi.fn(),
	tableProps: null as TableProps | null,
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
	useSearchParams: () => [mockState.searchParams, mockState.setSearchParams],
}));

vi.mock("@/i18n", () => ({
	default: {
		language: "en",
		t: (key: string) => key,
	},
}));

const testI18n = vi.hoisted(() => ({
	language: "en",
	resolvedLanguage: "en",
}));

const translate = vi.hoisted(() => (key: string) => key);

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		i18n: testI18n,
		t: translate,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/usePageTitle", () => ({ usePageTitle: vi.fn() }));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		actions,
		title,
	}: {
		actions?: React.ReactNode;
		title: string;
	}) => (
		<header>
			<h1>{title}</h1>
			{actions}
		</header>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/admin/AdminOffsetPagination", () => ({
	AdminOffsetPagination: () => null,
}));

vi.mock(
	"@/components/admin/admin-policies-page/StoragePolicyMigrationDialog",
	() => ({
		StoragePolicyMigrationDialog: () => null,
	}),
);

vi.mock("@/components/admin/admin-policies-page/PoliciesTable", () => ({
	PoliciesTable: (props: TableProps) => {
		mockState.tableProps = props;
		return (
			<div data-testid="policies-table">
				{props.policies.map((policy) => (
					<button
						type="button"
						key={policy.id}
						onClick={() => props.onEditPolicy(policy)}
					>
						edit:{policy.id}
					</button>
				))}
			</div>
		);
	},
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: () => null,
}));

vi.mock(
	"@/components/admin/storage-policy-editor/StoragePolicyEditorForm",
	() => ({
		StoragePolicyEditorForm: () => null,
	}),
);

vi.mock(
	"@/pages/admin/admin-policies-page/useStoragePolicyEditorWorkspace",
	() => ({
		useStoragePolicyEditorWorkspace: () => null,
	}),
);

vi.mock(
	"@/pages/admin/admin-policies-page/useStoragePolicyDescriptorController",
	() => ({
		useStoragePolicyDescriptorController: () => ({
			remoteNodes: [],
			storageDriverDescriptors: [],
			refreshLookups: vi.fn(async () => undefined),
		}),
	}),
);

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		createMigration: vi.fn(),
		delete: vi.fn(),
		dryRunMigration: vi.fn(),
		list: (...args: unknown[]) => mockState.list(...args),
		listStorageDriverDescriptors: (query?: { context?: string }) =>
			mockState.listStorageDriverDescriptors(query),
		listStorageDriverLocalizations: vi.fn().mockResolvedValue({
			requested_locale: "en",
			resources: [],
		}),
	},
	adminRemoteNodeService: {
		list: vi.fn().mockResolvedValue({ items: [], total: 0 }),
	},
}));

function policy(id: number, name: string): StoragePolicy {
	return {
		allowed_types: [],
		behavior: {},
		chunk_size: 5 * 1024 * 1024,
		connector_config: {
			connector_id: "local",
			format_version: 1,
			schema_version: 1,
			values: {},
		},
		connector_id: "local",
		created_at: "2026-08-04T00:00:00Z",
		id,
		is_default: false,
		max_file_size: 0,
		name,
		updated_at: "2026-08-04T00:00:00Z",
	};
}

describe("AdminPoliciesPage", () => {
	beforeEach(() => {
		invalidateAdminStorageDriverDescriptors();
		mockState.handleApiError.mockReset();
		mockState.list.mockReset();
		mockState.listStorageDriverDescriptors.mockReset();
		mockState.navigate.mockReset();
		mockState.searchParams = new URLSearchParams();
		mockState.setSearchParams.mockReset();
		mockState.tableProps = null;
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();

		mockState.list.mockResolvedValue({
			items: [policy(7, "Primary")],
			total: 1,
		});
		mockState.listStorageDriverDescriptors.mockResolvedValue([]);
	});

	it("navigates to the editor pages from the table and the create action", async () => {
		render(<AdminPoliciesPage />);

		await waitFor(() =>
			expect(screen.getByTestId("policies-table")).toBeInTheDocument(),
		);

		fireEvent.click(screen.getByRole("button", { name: /new_policy/ }));
		expect(mockState.navigate).toHaveBeenCalledWith("/admin/policies/new", {
			viewTransition: false,
		});

		fireEvent.click(screen.getByRole("button", { name: "edit:7" }));
		expect(mockState.navigate).toHaveBeenCalledWith("/admin/policies/7", {
			viewTransition: false,
		});
	});

	it("consumes an authorization callback, reloads, and routes to the editor", async () => {
		mockState.searchParams = new URLSearchParams(
			"storage_authorization=success&policy_id=7&keep=value",
		);

		render(<AdminPoliciesPage />);

		await waitFor(() => {
			expect(mockState.setSearchParams).toHaveBeenCalledWith(
				new URLSearchParams("keep=value"),
				{ replace: true },
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"storage_authorization_completed",
			expect.any(Object),
		);
		expect(mockState.navigate).toHaveBeenCalledWith("/admin/policies/7", {
			viewTransition: false,
		});
	});

	it("surfaces authorization callback failures without navigation", async () => {
		mockState.searchParams = new URLSearchParams(
			"storage_authorization=failed&reason=invalid_state",
		);

		render(<AdminPoliciesPage />);

		await waitFor(() => {
			expect(mockState.toastError).toHaveBeenCalledWith(
				"storage_authorization_failed_invalid_state",
			);
		});
		expect(mockState.navigate).not.toHaveBeenCalled();
	});
});

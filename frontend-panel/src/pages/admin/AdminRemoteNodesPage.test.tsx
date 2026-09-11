import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminRemoteNodesPage from "@/pages/admin/AdminRemoteNodesPage";

const mocks = vi.hoisted(() => ({
	handleRefresh: vi.fn(),
	openCreate: vi.fn(),
	openEdit: vi.fn(),
	handleGenerateEnrollmentCommand: vi.fn(),
	requestConfirm: vi.fn(),
}));

vi.mock("@/pages/admin/useAdminRemoteNodesPageController", () => ({
	useAdminRemoteNodesPageController: () => ({
		copyToClipboard: vi.fn(),
		createButtonTitle: undefined,
		currentPage: 1,
		deleteDialogProps: { open: false },
		deleteNodeName: "",
		deletingRemoteNodeId: null,
		enrollmentCommand: null,
		enrollmentCommandCanTest: false,
		handleGenerateEnrollmentCommand: mocks.handleGenerateEnrollmentCommand,
		handlePageSizeChange: vi.fn(),
		handleRefresh: mocks.handleRefresh,
		handleSortChange: vi.fn(),
		handleVerifyEnrollmentConnection: vi.fn(),
		loading: false,
		nextPageDisabled: true,
		openCreate: mocks.openCreate,
		openEdit: mocks.openEdit,
		pageSize: 20,
		pageSizeOptions: [],
		prevPageDisabled: true,
		remoteNodes: [{ id: 7, name: "Edge Alpha" }],
		requestConfirm: mocks.requestConfirm,
		setOffset: vi.fn(),
		sortBy: "created_at",
		sortOrder: "desc",
		t: (key: string) => key,
		total: 1,
		totalPages: 1,
	}),
}));

vi.mock("@/components/admin/admin-remote-nodes-page/RemoteNodesTable", () => ({
	RemoteNodesTable: ({
		items,
		onEdit,
		onRequestDelete,
		pagination,
	}: {
		items: Array<{ id: number; name: string }>;
		onEdit: (node: { id: number; name: string }) => void;
		onRequestDelete: (id: number) => void;
		pagination?: React.ReactNode;
	}) => (
		<div>
			{items.map((node) => (
				<div key={node.id}>
					<span>{node.name}</span>
					<button type="button" onClick={() => onEdit(node)}>
						open-node
					</button>
					<button type="button" onClick={() => onRequestDelete(node.id)}>
						delete-node
					</button>
				</div>
			))}
			{pagination}
		</div>
	),
}));
vi.mock("@/components/admin/AdminOffsetPagination", () => ({
	AdminOffsetPagination: () => <div>pagination</div>,
}));
vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: () => null,
}));
vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));
vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<>{children}</>
	),
}));
vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		actions,
		title,
	}: {
		actions: React.ReactNode;
		title: string;
	}) => (
		<>
			<h1>{title}</h1>
			{actions}
		</>
	),
}));
vi.mock("@/components/ui/icon", () => ({ Icon: () => null }));

describe("AdminRemoteNodesPage", () => {
	beforeEach(() => {
		Object.values(mocks).forEach((mock) => {
			mock.mockReset();
		});
	});

	it("renders the list workflow and delegates list actions", () => {
		render(<AdminRemoteNodesPage />);
		fireEvent.click(screen.getByRole("button", { name: "new_remote_node" }));
		fireEvent.click(screen.getByRole("button", { name: "core:refresh" }));
		fireEvent.click(screen.getByRole("button", { name: "open-node" }));
		fireEvent.click(screen.getByRole("button", { name: "delete-node" }));

		expect(screen.getByText("pagination")).toBeInTheDocument();
		expect(mocks.openCreate).toHaveBeenCalledTimes(1);
		expect(mocks.handleRefresh).toHaveBeenCalledTimes(1);
		expect(mocks.openEdit).toHaveBeenCalledWith({ id: 7, name: "Edge Alpha" });
		expect(mocks.requestConfirm).toHaveBeenCalledWith(7);
	});
});

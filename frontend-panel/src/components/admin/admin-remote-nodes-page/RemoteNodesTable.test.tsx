import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RemoteNodesTable } from "@/components/admin/admin-remote-nodes-page/RemoteNodesTable";
import type { RemoteNodeInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/components/ui/icon", () => ({ Icon: () => null }));
vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));
vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
		disabled,
		...props
	}: {
		children: React.ReactNode;
		onClick?: () => void;
		disabled?: boolean;
		[key: string]: unknown;
	}) => (
		<button onClick={onClick} disabled={disabled} {...props}>
			{children}
		</button>
	),
}));
vi.mock("@/components/common/AdminTable", async () => {
	const React = await import("react");
	return {
		ADMIN_INTERACTIVE_TABLE_ROW_CLASS: "row",
		ADMIN_TABLE_BADGE_CELL_CLASS: "badge-cell",
		ADMIN_TABLE_MONO_TEXT_CLASS: "mono",
		ADMIN_TABLE_TEXT_CELL_CLASS: "text-cell",
		AdminSortableTableHead: ({ children }: { children: React.ReactNode }) => (
			<th>{children}</th>
		),
		AdminTableCell: ({
			children,
			...props
		}: {
			children: React.ReactNode;
			[key: string]: unknown;
		}) => <td {...props}>{children}</td>,
		AdminTableHead: ({ children }: { children: React.ReactNode }) => (
			<th>{children}</th>
		),
		AdminTableHeader: ({ children }: { children: React.ReactNode }) => (
			<thead>{children}</thead>
		),
		AdminTableRow: ({
			children,
			...props
		}: {
			children: React.ReactNode;
			[key: string]: unknown;
		}) => <tr {...props}>{children}</tr>,
	};
});
vi.mock("@/components/common/AdminTableList", () => ({
	AdminTableList: ({
		headerRow,
		items,
		renderRow,
		pagination,
	}: {
		headerRow: React.ReactNode;
		items: RemoteNodeInfo[];
		renderRow: (node: RemoteNodeInfo) => React.ReactNode;
		pagination?: React.ReactNode;
	}) => (
		<>
			<table>
				{headerRow}
				<tbody>{items.map(renderRow)}</tbody>
			</table>
			{pagination}
		</>
	),
}));

const node: RemoteNodeInfo = {
	id: 13,
	name: "test",
	base_url: "",
	is_enabled: true,
	enrollment_status: "pending",
	last_probe_at: null,
	last_probe_error: "",
	capabilities: {
		protocol_version: "v1",
		supports_list: true,
		supports_range_read: true,
		supports_stream_upload: true,
	},
	created_at: "",
	updated_at: "",
};

describe("RemoteNodesTable", () => {
	it("keeps the operation column limited to deletion", () => {
		const onEdit = vi.fn();
		render(
			<RemoteNodesTable
				items={[node]}
				loading={false}
				deletingRemoteNodeId={null}
				onEdit={onEdit}
				onRequestDelete={vi.fn()}
				sortBy="id"
				sortOrder="asc"
				onSortChange={vi.fn()}
			/>,
		);
		fireEvent.click(screen.getByText("test"));
		expect(onEdit).toHaveBeenCalledWith(node);
		expect(
			screen.queryByText("remote_node_generate_enrollment_command"),
		).not.toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "delete_remote_node" }),
		).toBeInTheDocument();
	});
});

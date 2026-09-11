import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { RemoteNodesTable } from "@/components/admin/admin-remote-nodes-page/RemoteNodesTable";
import type { RemoteNodeInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/components/common/AdminTable", () => ({
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS: "interactive-row",
	ADMIN_TABLE_BADGE_CELL_CLASS: "badge-cell",
	ADMIN_TABLE_MONO_TEXT_CLASS: "mono-cell",
	ADMIN_TABLE_TEXT_CELL_CLASS: "text-cell",
	ADMIN_TABLE_ACTIONS_WIDTH_CLASS: "actions-cell",
	AdminSortableTableHead: ({ children }: { children: ReactNode }) => (
		<th>{children}</th>
	),
	AdminTableCell: ({ children, ...props }: { children: ReactNode }) => (
		<td {...props}>{children}</td>
	),
	AdminTableHead: ({ children }: { children: ReactNode }) => (
		<th>{children}</th>
	),
	AdminTableHeader: ({ children }: { children: ReactNode }) => (
		<thead>{children}</thead>
	),
	AdminTableRow: ({ children, ...props }: { children: ReactNode }) => (
		<tr {...props}>{children}</tr>
	),
}));

vi.mock("@/components/common/AdminTableList", () => ({
	AdminTableList: ({
		emptyTitle,
		headerRow,
		items,
		renderRow,
	}: {
		emptyTitle: string;
		headerRow: ReactNode;
		items: RemoteNodeInfo[];
		renderRow: (item: RemoteNodeInfo) => ReactNode;
	}) => (
		<div>
			<table>
				{headerRow}
				<tbody>{items.map(renderRow)}</tbody>
			</table>
			{items.length === 0 ? <p>{emptyTitle}</p> : null}
		</div>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: ReactNode }) => <span>{children}</span>,
}));
vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
		...props
	}: {
		children: ReactNode;
		onClick?: () => void;
	}) => (
		<button type="button" onClick={onClick} {...props}>
			{children}
		</button>
	),
}));
vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

function node(overrides: Partial<RemoteNodeInfo> = {}): RemoteNodeInfo {
	return {
		id: 7,
		name: "Edge Alpha",
		base_url: "https://edge.example.test",
		is_enabled: true,
		enrollment_status: "completed",
		last_probe_error: "",
		last_probe_at: null,
		capabilities: null,
		created_at: "2026-09-11T00:00:00Z",
		updated_at: "2026-09-11T00:00:00Z",
		transport_mode: "direct",
		...overrides,
	};
}

describe("RemoteNodesTable", () => {
	it("renders empty and populated rows, including delete state and keyboard edit", () => {
		const onEdit = vi.fn();
		const onRequestDelete = vi.fn();
		const empty = render(
			<RemoteNodesTable
				deletingRemoteNodeId={null}
				items={[]}
				loading={false}
				onEdit={onEdit}
				onRequestDelete={onRequestDelete}
				sortBy="created_at"
				sortOrder="desc"
				onSortChange={vi.fn()}
			/>,
		);
		expect(screen.getByText("no_remote_nodes")).toBeVisible();
		empty.unmount();

		const item = node({
			tunnel: {
				status: "offline",
				runtime_error: "tunnel failed",
				last_handshake_at: null,
			} as never,
		});
		const deleting = render(
			<RemoteNodesTable
				deletingRemoteNodeId={7}
				items={[item]}
				loading={false}
				onEdit={onEdit}
				onRequestDelete={onRequestDelete}
				sortBy="created_at"
				sortOrder="desc"
				onSortChange={vi.fn()}
			/>,
		);
		fireEvent.click(
			screen.getByRole("button", { name: "remote_node_deleting" }),
		);
		expect(onRequestDelete).not.toHaveBeenCalled();
		expect(screen.getByText("Spinner")).toBeVisible();

		const row = screen.getByText("Edge Alpha").closest("tr");
		expect(row).not.toBeNull();
		if (row) fireEvent.keyDown(row, { key: "Enter" });
		expect(onEdit).not.toHaveBeenCalled();
		deleting.unmount();
		const active = render(
			<RemoteNodesTable
				deletingRemoteNodeId={null}
				items={[node({ base_url: "", transport_mode: undefined })]}
				loading={false}
				onEdit={onEdit}
				onRequestDelete={onRequestDelete}
				sortBy="created_at"
				sortOrder="desc"
				onSortChange={vi.fn()}
			/>,
		);
		const activeRow = screen.getByText("Edge Alpha").closest("tr");
		if (activeRow) fireEvent.keyDown(activeRow, { key: "Enter" });
		fireEvent.click(screen.getByRole("button", { name: "delete_remote_node" }));
		expect(onEdit).toHaveBeenCalledWith(expect.objectContaining({ id: 7 }));
		expect(onRequestDelete).toHaveBeenCalledWith(7);
		active.unmount();
	});
});

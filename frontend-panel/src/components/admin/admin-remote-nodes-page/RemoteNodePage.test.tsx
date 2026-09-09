import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RemoteNodePage } from "@/components/admin/admin-remote-nodes-page/RemoteNodePage";
import type { RemoteNodeInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/components/layout/AdminDetailPageShell", () => ({
	AdminDetailPageShell: ({
		actions,
		children,
		title,
	}: {
		actions: React.ReactNode;
		children: React.ReactNode;
		title: string;
	}) => (
		<div>
			<h1>{title}</h1>
			<div>{actions}</div>
			{children}
		</div>
	),
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
		disabled,
		form,
		onClick,
		type,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		form?: string;
		onClick?: () => void;
		type?: string;
	}) => (
		<button
			disabled={disabled}
			form={form}
			onClick={onClick}
			type={type as "button" | "submit" | undefined}
		>
			{children}
		</button>
	),
}));
vi.mock("@/components/ui/input", () => ({
	Input: (props: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));
vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
		...props
	}: React.LabelHTMLAttributes<HTMLLabelElement>) => (
		<label htmlFor={htmlFor} {...props}>
			{children}
		</label>
	),
}));
vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		id,
		checked,
		onCheckedChange,
	}: {
		id?: string;
		checked?: boolean;
		onCheckedChange?: (value: boolean) => void;
	}) => (
		<input
			id={id}
			type="checkbox"
			checked={checked}
			onChange={(event) => onCheckedChange?.(event.target.checked)}
		/>
	),
}));
vi.mock(
	"@/components/admin/admin-remote-nodes-page/RemoteNodeRemoteStorageTargetSection",
	() => ({ RemoteNodeRemoteStorageTargetSection: () => null }),
);

const baseProps = {
	baseUrlValidationMessage: null,
	editingNode: null,
	form: {
		name: "",
		base_url: "",
		transport_mode: "direct" as const,
		is_enabled: true,
	},
	onBack: vi.fn(),
	onFieldChange: vi.fn(),
	onRunConnectionTest: vi.fn(async () => true),
	onSubmit: vi.fn(),
	pageBackLabel: "back_to_remote_nodes",
	submitting: false,
};

const node = (overrides: Partial<RemoteNodeInfo> = {}): RemoteNodeInfo => ({
	id: 7,
	name: "Edge Alpha",
	base_url: "https://edge.example.com",
	is_enabled: true,
	enrollment_status: "not_started",
	last_probe_error: "",
	last_probe_at: null,
	capabilities: {
		protocol_version: "v1",
		supports_list: true,
		supports_range_read: true,
		supports_stream_upload: true,
	},
	created_at: "",
	updated_at: "",
	...overrides,
});

describe("RemoteNodePage", () => {
	it("renders the single-page create form with transport choices and deploy action", () => {
		render(<RemoteNodePage {...baseProps} mode="create" />);
		expect(
			screen.getByRole("heading", { name: "create_remote_node" }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("radiogroup", { name: "remote_node_transport_mode" }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "remote_node_create_and_deploy" }),
		).toBeDisabled();
		expect(
			screen.getByRole("link", { name: "remote_node_wizard_docs_link" }),
		).toBeInTheDocument();
	});

	it("keeps edit connection testing disabled until the saved connection is unchanged and healthy", () => {
		const onRunConnectionTest = vi.fn(async () => true);
		render(
			<RemoteNodePage
				{...baseProps}
				editingNode={node({ enrollment_status: "completed" })}
				form={{
					...baseProps.form,
					name: "Edge Alpha",
					base_url: "https://edge.example.com",
				}}
				mode="edit"
				onRunConnectionTest={onRunConnectionTest}
			/>,
		);
		const testButton = screen.getByRole("button", { name: "test_connection" });
		expect(testButton).not.toBeDisabled();
		fireEvent.click(testButton);
		expect(onRunConnectionTest).toHaveBeenCalledTimes(1);
	});
});

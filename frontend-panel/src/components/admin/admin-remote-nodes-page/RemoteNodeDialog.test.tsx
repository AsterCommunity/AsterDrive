import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RemoteNodeDialog } from "@/components/admin/admin-remote-nodes-page/RemoteNodeDialog";
import type { RemoteNodeInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <span className={className}>{children}</span>,
}));

vi.mock("@/components/ui/button", async (importOriginal) => {
	const actual =
		await importOriginal<typeof import("@/components/ui/button")>();
	return {
		...actual,
		Button: ({
			children,
			type,
			disabled,
			onClick,
			className,
			...props
		}: {
			children: React.ReactNode;
			type?: "button" | "submit";
			disabled?: boolean;
			onClick?: () => void;
			className?: string;
			[key: string]: unknown;
		}) => (
			<button
				type={type ?? "button"}
				disabled={disabled}
				onClick={onClick}
				className={className}
				{...props}
			>
				{children}
			</button>
		),
	};
});

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogFooter: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogHeader: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span aria-hidden="true" data-icon-name={name} />
	),
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		id,
		value,
		onChange,
		placeholder,
		className,
		...props
	}: {
		id?: string;
		value?: string;
		onChange?: (event: React.ChangeEvent<HTMLInputElement>) => void;
		placeholder?: string;
		className?: string;
		[key: string]: unknown;
	}) => (
		<input
			id={id}
			value={value}
			onChange={onChange}
			placeholder={placeholder}
			className={className}
			{...props}
		/>
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
		...props
	}: {
		children: React.ReactNode;
		htmlFor?: string;
		[key: string]: unknown;
	}) => (
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

const baseProps = {
	createStep: 0,
	createStepTouched: false,
	editingNode: null,
	form: {
		name: "",
		base_url: "",
		transport_mode: "direct",
		is_enabled: true,
	},
	onCreateBack: vi.fn(),
	onCreateNext: vi.fn(),
	onCreateStepChange: vi.fn(),
	onFieldChange: vi.fn(),
	onOpenChange: vi.fn(),
	onRunConnectionTest: vi.fn(async () => true),
	onSubmit: vi.fn(),
	open: true,
	submitting: false,
} as const;

const remoteNode = (
	overrides: Partial<RemoteNodeInfo> = {},
): RemoteNodeInfo => ({
	id: 7,
	name: "Edge Alpha",
	base_url: "https://edge.example.com",
	is_enabled: true,
	enrollment_status: "not_started",
	last_error: "",
	last_checked_at: null,
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

describe("RemoteNodeDialog", () => {
	it("shows the docker follower docs link in create mode", () => {
		render(<RemoteNodeDialog {...baseProps} mode="create" />);

		expect(
			screen.getByRole("link", { name: "remote_node_wizard_docs_link" }),
		).toHaveAttribute(
			"href",
			"https://drive.docs.astercosm.com/deploy/follower-node/",
		);
	});

	it("hides the docker follower docs link in edit mode", () => {
		render(<RemoteNodeDialog {...baseProps} mode="edit" />);

		expect(
			screen.queryByRole("link", { name: "remote_node_wizard_docs_link" }),
		).not.toBeInTheDocument();
	});

	it("disables connection tests before remote node enrollment completes", () => {
		const onRunConnectionTest = vi.fn(async () => true);

		render(
			<RemoteNodeDialog
				{...baseProps}
				mode="edit"
				editingNode={remoteNode({ enrollment_status: "pending" })}
				form={{
					name: "Edge Alpha",
					base_url: "https://edge.example.com",
					transport_mode: "direct",
					is_enabled: true,
				}}
				onRunConnectionTest={onRunConnectionTest}
			/>,
		);

		const button = screen.getByRole("button", { name: "test_connection" });
		expect(button).toBeDisabled();

		fireEvent.click(button);

		expect(onRunConnectionTest).not.toHaveBeenCalled();
	});

	it("allows connection tests after remote node enrollment completes", () => {
		const onRunConnectionTest = vi.fn(async () => true);

		render(
			<RemoteNodeDialog
				{...baseProps}
				mode="edit"
				editingNode={remoteNode({ enrollment_status: "completed" })}
				form={{
					name: "Edge Alpha",
					base_url: "https://edge.example.com",
					transport_mode: "direct",
					is_enabled: true,
				}}
				onRunConnectionTest={onRunConnectionTest}
			/>,
		);

		const button = screen.getByRole("button", { name: "test_connection" });
		expect(button).not.toBeDisabled();

		fireEvent.click(button);

		expect(onRunConnectionTest).toHaveBeenCalledTimes(1);
	});

	it("shows the reverse tunnel test badge and explicit auto semantics in create mode", () => {
		render(<RemoteNodeDialog {...baseProps} createStep={1} mode="create" />);

		expect(
			screen.getByRole("radiogroup", {
				name: "remote_node_transport_mode",
			}),
		).toBeInTheDocument();
		expect(
			screen.getByRole("radio", { name: /remote_node_transport_direct/ }),
		).toBeChecked();
		expect(
			screen.getByText("remote_node_transport_test_badge"),
		).toBeInTheDocument();
		expect(
			screen.getByText("remote_node_transport_auto_desc"),
		).toBeInTheDocument();
		expect(screen.getByText("remote_node_base_url_hint")).toBeInTheDocument();
	});

	it("shows create-step validation messages and blocks invalid connection URLs", () => {
		const onCreateNext = vi.fn();

		const { rerender } = render(
			<RemoteNodeDialog
				{...baseProps}
				createStepTouched
				mode="create"
				onCreateNext={onCreateNext}
			/>,
		);

		expect(
			screen.getByText("remote_node_wizard_name_required"),
		).toBeInTheDocument();

		rerender(
			<RemoteNodeDialog
				{...baseProps}
				createStep={1}
				form={{
					name: "Edge Alpha",
					base_url: "ftp://edge.example.com",
					transport_mode: "direct",
					is_enabled: true,
				}}
				mode="create"
				onCreateNext={onCreateNext}
			/>,
		);

		expect(
			screen.getByText("remote_node_base_url_invalid"),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		).toBeDisabled();
	});

	it("shows the reverse tunnel test badge in edit mode", () => {
		render(
			<RemoteNodeDialog
				{...baseProps}
				mode="edit"
				editingNode={remoteNode({
					enrollment_status: "completed",
					transport_mode: "reverse_tunnel",
				})}
				form={{
					name: "Edge Alpha",
					base_url: "",
					transport_mode: "reverse_tunnel",
					is_enabled: true,
				}}
			/>,
		);

		expect(
			screen.getAllByText("remote_node_transport_test_badge").length,
		).toBeGreaterThan(0);
	});

	it("falls back invalid transport form values to direct and blocks invalid edits", () => {
		const onSubmit = vi.fn();

		render(
			<RemoteNodeDialog
				{...baseProps}
				mode="edit"
				editingNode={remoteNode({
					enrollment_status: "completed",
					transport_mode: "direct",
				})}
				form={
					{
						name: "Edge Alpha",
						base_url: "mailto:edge@example.com",
						transport_mode: "legacy",
						is_enabled: true,
					} as typeof baseProps.form
				}
				onSubmit={onSubmit}
			/>,
		);

		expect(
			screen.getAllByText("remote_node_transport_direct").length,
		).toBeGreaterThan(0);
		expect(
			screen.getByText("remote_node_base_url_invalid"),
		).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "save_changes" })).toBeDisabled();
	});
});

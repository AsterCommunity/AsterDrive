import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RemoteNodeRemoteStorageTargetSection } from "@/components/admin/admin-remote-nodes-page/RemoteNodeRemoteStorageTargetSection";
import type {
	RemoteStorageTargetConnectorDescriptor,
	RemoteStorageTargetInfo,
} from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options?.name ? `${key}:${options.name}` : key,
	}),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: ReactNode }) => <span>{children}</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({ children, ...props }: { children: ReactNode }) => (
		<button type="button" {...props}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name, ...props }: { name: string }) => (
		<span {...props}>{name}</span>
	),
}));

vi.mock("@/components/ui/input", () => ({
	Input: (props: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({ children, htmlFor }: { children: ReactNode; htmlFor?: string }) => (
		<label htmlFor={htmlFor}>{children}</label>
	),
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		items = [],
		onValueChange,
		value,
	}: {
		children: ReactNode;
		items?: Array<{ label: string; value: string }>;
		onValueChange?: (value: string) => void;
		value: string;
	}) => (
		<div>
			<select
				aria-label={`select:${value}`}
				value={value}
				onChange={(event) => onValueChange?.(event.currentTarget.value)}
			>
				{items.map((item) => (
					<option key={item.value} value={item.value}>
						{item.label}
					</option>
				))}
			</select>
			{children}
		</div>
	),
	SelectContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({ children }: { children: ReactNode }) => <div>{children}</div>,
	SelectTrigger: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectValue: () => null,
}));

vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		onCheckedChange,
		...props
	}: React.InputHTMLAttributes<HTMLInputElement> & {
		onCheckedChange?: (value: boolean) => void;
	}) => (
		<input
			{...props}
			type="checkbox"
			onChange={(event) => onCheckedChange?.(event.currentTarget.checked)}
		/>
	),
}));

vi.mock("@/lib/format", () => ({
	formatDateTime: (value: string) => `date:${value}`,
}));

const LOCAL_CONNECTOR = "asterdrive.remote-target.local";
const ARCHIVE_CONNECTOR = "plugin.example.archive";

const localDescriptor: RemoteStorageTargetConnectorDescriptor = {
	config_schema_version: 1,
	connector_id: LOCAL_CONNECTOR,
	credential_schema_version: null,
	description_key: "local_description",
	fields: [
		{
			kind: "text",
			label_key: "base_path",
			name: "base_path",
			placeholder: "tenant/incoming",
			required: true,
			scope: "connector_config",
			secret: false,
		},
	],
	label_key: "local_label",
};

const archiveDescriptor: RemoteStorageTargetConnectorDescriptor = {
	config_schema_version: 3,
	connector_id: ARCHIVE_CONNECTOR,
	credential_schema_version: 2,
	description_key: "archive_description",
	fields: [
		{
			kind: "text",
			label_key: "endpoint",
			name: "endpoint",
			required: true,
			required_message_key: "endpoint_required",
			scope: "connector_config",
			secret: false,
		},
		{
			kind: "secret",
			label_key: "token",
			name: "token",
			required: true,
			scope: "static_credential",
			secret: true,
		},
		{
			default_value: true,
			kind: "boolean",
			label_key: "compress",
			name: "compress",
			required: false,
			scope: "connector_config",
			secret: false,
		},
		{
			default_value: 4,
			kind: "number",
			label_key: "workers",
			name: "workers",
			required: true,
			scope: "connector_config",
			secret: false,
			validation: { max_integer: 8, min_integer: 1 },
		},
		{
			default_value: "zstd",
			kind: "select",
			label_key: "codec",
			name: "codec",
			required: true,
			scope: "connector_config",
			secret: false,
			select: {
				options: [
					{ label_key: "codec_zstd", value: "zstd" },
					{ label_key: "codec_lz4", value: "lz4" },
				],
				value_kind: "string",
			},
		},
	],
	label_key: "archive_label",
};

const descriptors = [localDescriptor, archiveDescriptor];

const target = (
	overrides: Partial<RemoteStorageTargetInfo> = {},
): RemoteStorageTargetInfo => ({
	applied_revision: 2,
	connector_available: true,
	connector_config: {
		connector_id: LOCAL_CONNECTOR,
		format_version: 1,
		schema_version: 1,
		values: { base_path: "incoming" },
	},
	connector_id: LOCAL_CONNECTOR,
	created_at: "2026-05-01T00:00:00Z",
	credential_configured: false,
	desired_revision: 2,
	is_default: false,
	last_error: "",
	name: "Local ingress",
	target_key: "local-default",
	updated_at: "2026-05-02T00:00:00Z",
	...overrides,
});

function renderSection({
	allowCreate = false,
	connectorDescriptors = descriptors,
	onCreateTarget = vi.fn().mockResolvedValue(undefined),
	onDeleteTarget = vi.fn().mockResolvedValue(undefined),
	onUpdateTarget = vi.fn().mockResolvedValue(undefined),
	readOnly = false,
	targets = [] as RemoteStorageTargetInfo[],
} = {}) {
	render(
		<RemoteNodeRemoteStorageTargetSection
			allowCreate={allowCreate}
			connectorDescriptors={connectorDescriptors}
			errorMessage={null}
			loading={false}
			onCreateTarget={onCreateTarget}
			onDeleteTarget={onDeleteTarget}
			onUpdateTarget={onUpdateTarget}
			readOnly={readOnly}
			targets={targets}
		/>,
	);
	return { onCreateTarget, onDeleteTarget, onUpdateTarget };
}

function beginCreate() {
	fireEvent.click(
		screen.getByRole("button", {
			name: "remote_node_ingress_profiles_create",
		}),
	);
}

describe("RemoteNodeRemoteStorageTargetSection", () => {
	beforeEach(() => vi.clearAllMocks());

	it("keeps the read-only list collapsed and hides management actions", async () => {
		const user = userEvent.setup();
		renderSection({ readOnly: true, targets: [target()] });

		const toggle = screen.getByRole("button", {
			name: "policy_remote_storage_targets_show",
		});
		expect(toggle).toHaveAttribute("aria-expanded", "false");
		expect(screen.queryByText("Local ingress")).not.toBeInTheDocument();
		await user.click(toggle);
		expect(screen.getByText("Local ingress")).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "core:edit" })).toBeNull();
		expect(screen.queryByRole("button", { name: "core:delete" })).toBeNull();
	});

	it("creates a target with a generic connector envelope", async () => {
		const { onCreateTarget } = renderSection();
		beginCreate();
		expect(
			screen.getByLabelText("remote_node_ingress_profile_default_toggle"),
		).toBeChecked();
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: " Local upload " },
		});
		fireEvent.change(screen.getByLabelText("base_path"), {
			target: { value: " teams/incoming " },
		});
		fireEvent.click(screen.getByRole("button", { name: /core:create/ }));

		await waitFor(() =>
			expect(onCreateTarget).toHaveBeenCalledWith({
				connector_config: {
					connector_id: LOCAL_CONNECTOR,
					format_version: 1,
					schema_version: 1,
					values: { base_path: "teams/incoming" },
				},
				credential: undefined,
				is_default: true,
				name: "Local upload",
			}),
		);
	});

	it("renders descriptor defaults and validates text, secret, number, boolean and select fields", async () => {
		const { onCreateTarget } = renderSection({ targets: [target()] });
		beginCreate();
		fireEvent.change(screen.getByLabelText(`select:${LOCAL_CONNECTOR}`), {
			target: { value: ARCHIVE_CONNECTOR },
		});

		expect(screen.getByLabelText("compress")).toBeChecked();
		expect(screen.getByLabelText("workers")).toHaveValue(4);
		expect(screen.getByLabelText("select:zstd")).toHaveValue("zstd");
		expect(screen.getByText("endpoint_required")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /core:create/ })).toBeDisabled();

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Archive" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://archive.test" },
		});
		fireEvent.change(screen.getByLabelText("token"), {
			target: { value: " TOKEN " },
		});
		fireEvent.change(screen.getByLabelText("workers"), {
			target: { value: "9" },
		});
		expect(
			screen.getByText("remote_node_ingress_profile_field_invalid_number"),
		).toBeInTheDocument();
		fireEvent.change(screen.getByLabelText("workers"), {
			target: { value: "6" },
		});
		fireEvent.change(screen.getByLabelText("select:zstd"), {
			target: { value: "lz4" },
		});
		fireEvent.click(screen.getByLabelText("compress"));
		fireEvent.click(screen.getByRole("button", { name: /core:create/ }));

		await waitFor(() =>
			expect(onCreateTarget).toHaveBeenCalledWith({
				connector_config: {
					connector_id: ARCHIVE_CONNECTOR,
					format_version: 1,
					schema_version: 3,
					values: {
						codec: "lz4",
						compress: false,
						endpoint: "https://archive.test",
						workers: 6,
					},
				},
				credential: { mode: "static", values: { token: "TOKEN" } },
				is_default: false,
				name: "Archive",
			}),
		);
	});

	it("preserves an existing secret when editing the same connector", async () => {
		const existing = target({
			connector_config: {
				connector_id: ARCHIVE_CONNECTOR,
				format_version: 1,
				schema_version: 3,
				values: {
					codec: "zstd",
					compress: true,
					endpoint: "https://archive.test",
					workers: 4,
				},
			},
			connector_id: ARCHIVE_CONNECTOR,
			credential_configured: true,
			name: "Archive",
			target_key: "archive",
		});
		const { onUpdateTarget } = renderSection({ targets: [existing] });
		fireEvent.click(screen.getByRole("button", { name: "core:edit" }));
		expect(screen.getByLabelText("token")).toHaveAttribute(
			"placeholder",
			"••••••••",
		);
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Archive renamed" },
		});
		fireEvent.click(screen.getByRole("button", { name: /save_changes/ }));

		await waitFor(() =>
			expect(onUpdateTarget).toHaveBeenCalledWith(
				"archive",
				expect.objectContaining({
					credential: undefined,
					name: "Archive renamed",
				}),
			),
		);
	});

	it("requires a new credential after switching connectors", () => {
		renderSection({ targets: [target()] });
		fireEvent.click(screen.getByRole("button", { name: "core:edit" }));
		fireEvent.change(screen.getByLabelText(`select:${LOCAL_CONNECTOR}`), {
			target: { value: ARCHIVE_CONNECTOR },
		});
		expect(screen.getByRole("button", { name: /save_changes/ })).toBeDisabled();
		expect(screen.getByText("endpoint_required")).toBeInTheDocument();
		expect(
			screen.getByText("remote_node_ingress_profile_field_required"),
		).toBeInTheDocument();
	});

	it("retains unavailable connector data and disables edit", () => {
		renderSection({
			targets: [
				target({
					connector_available: false,
					connector_config: {
						connector_id: "plugin.missing",
						format_version: 1,
						schema_version: 9,
						values: { opaque: "retained" },
					},
					connector_id: "plugin.missing",
					last_error: "connector unavailable",
				}),
			],
		});
		expect(screen.getByText("plugin.missing")).toBeInTheDocument();
		expect(screen.getByText("connector unavailable")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "core:edit" })).toBeDisabled();
		expect(screen.getByRole("button", { name: "core:delete" })).toBeEnabled();
	});

	it("confirms deletion and calls the delete handler", async () => {
		const existing = target();
		const { onDeleteTarget } = renderSection({ targets: [existing] });
		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));
		const deleteButtons = screen.getAllByRole("button", {
			name: "core:delete",
		});
		expect(deleteButtons).toHaveLength(1);
		fireEvent.click(deleteButtons[0]);
		await waitFor(() => expect(onDeleteTarget).toHaveBeenCalledWith(existing));
	});

	it("does not expose creation when descriptors or handler are missing", () => {
		const { rerender } = render(
			<RemoteNodeRemoteStorageTargetSection
				connectorDescriptors={[]}
				errorMessage={null}
				loading={false}
				onCreateTarget={vi.fn()}
				targets={[]}
			/>,
		);
		expect(
			screen.queryByRole("button", {
				name: "remote_node_ingress_profiles_create",
			}),
		).toBeNull();

		rerender(
			<RemoteNodeRemoteStorageTargetSection
				connectorDescriptors={descriptors}
				errorMessage={null}
				loading={false}
				targets={[]}
			/>,
		);
		expect(
			screen.queryByRole("button", {
				name: "remote_node_ingress_profiles_create",
			}),
		).toBeNull();
	});
});

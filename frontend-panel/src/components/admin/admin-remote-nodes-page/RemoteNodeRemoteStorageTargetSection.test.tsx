import {
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RemoteNodeRemoteStorageTargetSection } from "@/components/admin/admin-remote-nodes-page/RemoteNodeRemoteStorageTargetSection";
import type {
	RemoteStorageTargetDriverDescriptor,
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
	Button: ({
		children,
		disabled,
		onClick,
		type,
		...props
	}: {
		children: ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button
			{...props}
			type={type ?? "button"}
			disabled={disabled}
			onClick={onClick}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({
		"aria-hidden": ariaHidden,
		name,
	}: {
		"aria-hidden"?: boolean;
		name: string;
	}) => <span aria-hidden={ariaHidden}>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		id,
		onChange,
		placeholder,
		type,
		value,
		...props
	}: {
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		type?: string;
		value?: string;
	}) => (
		<input
			{...props}
			id={id}
			placeholder={placeholder}
			type={type ?? "text"}
			value={value}
			onChange={(event) =>
				onChange?.({ target: { value: event.currentTarget.value } })
			}
		/>
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
		onValueChange,
		value,
	}: {
		children: ReactNode;
		onValueChange?: (value: string) => void;
		value: string;
	}) => (
		<div>
			<select
				aria-label={`select:${value}`}
				value={value}
				onChange={(event) => onValueChange?.(event.currentTarget.value)}
			>
			<option value="asterdrive.storage.local">local</option>
			<option value="asterdrive.storage.s3">s3</option>
				<option value="__all__">__all__</option>
			</select>
			{children}
		</div>
	),
	SelectContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({ children, value }: { children: ReactNode; value: string }) => (
		<div data-value={value}>{children}</div>
	),
	SelectTrigger: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectValue: () => null,
}));

vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		checked,
		disabled,
		id,
		onCheckedChange,
	}: {
		checked: boolean;
		disabled?: boolean;
		id?: string;
		onCheckedChange?: (value: boolean) => void;
	}) => (
		<input
			id={id}
			checked={checked}
			disabled={disabled}
			type="checkbox"
			onChange={(event) => onCheckedChange?.(event.currentTarget.checked)}
		/>
	),
}));

vi.mock("@/lib/format", () => ({
	formatDateTime: (value: string) => `date:${value}`,
}));

const profile = (
	overrides: Partial<RemoteStorageTargetInfo> = {},
): RemoteStorageTargetInfo => ({
	applied_revision: 2,
	base_path: "incoming",
	bucket: "",
	created_at: "2026-05-01T00:00:00Z",
	desired_revision: 2,
	driver_type: "local",
	endpoint: "",
	is_default: false,
	last_error: "",
	name: "Local ingress",
	target_key: "local-default",
	updated_at: "2026-05-02T00:00:00Z",
	connector_id: "asterdrive.storage.local",
	...overrides,
});

const localDriverDescriptor: RemoteStorageTargetDriverDescriptor = {
	connector_id: "asterdrive.storage.local",
	label: "Local",
	description: "Local storage",
	ui: {
		label_key: "remote_node_ingress_profile_driver_local",
		description_key: "remote_node_ingress_profile_local_scope_hint",
		badge_rgb: { red: 16, green: 185, blue: 129 },
		helper_key: "remote_node_ingress_profile_local_scope_hint",
		config_step_title_key: "remote_node_ingress_profile_driver_local",
		config_step_description_key: "remote_node_ingress_profile_local_scope_hint",
		edit_context_key: "remote_node_ingress_profile_local_scope_hint",
		base_path_empty_display: ".",
		base_path_placeholder: "tenant-a/incoming",
	},
	credential_mode: "none",
	deployment_scope: "instance_local",
	supports_initial_setup: true,
	requires_authorization: false,
	capabilities: { efficient_range: true, capacity: false, list: true, presigned_download: false, storage_native_thumbnail: false, storage_native_media_metadata: false, remote_node_binding: false, object_storage_transfer_strategy: false, object_naming: "opaque_uuid" },
	upload_workflows: { simple_upload: true, simple_upload_capabilities: { server_side_relay: true, policy_limited: true }, stream_upload: true, object_multipart_upload: false, provider_resumable_upload: false, presigned_upload: false, frontend_direct_provider_resumable_upload: false },
	config_schema_version: 1,
	fields: [
		{
			help_key: "remote_node_ingress_profile_local_path_hint",
			kind: "text",
			label_key: "base_path",
			name: "base_path",
			placeholder: "tenant-a/incoming",
			required: true,
			secret: false,
		},
		{
			help_key: "remote_node_ingress_profile_default_hint",
			kind: "boolean",
			label_key: "remote_node_ingress_profile_default_toggle",
			name: "is_default",
			placeholder: null,
			required: false,
			secret: false,
		},
	],
	actions: [],
	promotions: [],
	related_issues: [],
};

const s3DriverDescriptor: RemoteStorageTargetDriverDescriptor = {
	connector_id: "asterdrive.storage.s3",
	label: "S3",
	description: "S3 storage",
	ui: {
		label_key: "remote_node_ingress_profile_driver_s3",
		description_key: "remote_node_ingress_profile_s3_path_hint",
		badge_rgb: { red: 59, green: 130, blue: 246 },
		helper_key: "remote_node_ingress_profile_s3_path_hint",
		config_step_title_key: "remote_node_ingress_profile_driver_s3",
		config_step_description_key: "remote_node_ingress_profile_s3_path_hint",
		edit_context_key: "remote_node_ingress_profile_s3_path_hint",
		base_path_empty_display: ".",
		base_path_placeholder: "prefix",
	},
	credential_mode: "static_secret",
	deployment_scope: "shared_across_primary_instances",
	supports_initial_setup: true,
	requires_authorization: false,
	capabilities: { efficient_range: true, capacity: true, list: true, presigned_download: true, storage_native_thumbnail: false, storage_native_media_metadata: false, remote_node_binding: false, object_storage_transfer_strategy: true, object_naming: "opaque_uuid" },
	upload_workflows: { simple_upload: true, simple_upload_capabilities: { server_side_relay: true, policy_limited: true }, stream_upload: true, object_multipart_upload: true, object_multipart_upload_capabilities: { min_part_size: 1, policy_limited_part_size: true, relay_part_upload: true, presigned_part_upload: true, presigned_part_etag_required: true, explicit_complete_required: true, abort_supported: true }, provider_resumable_upload: false, presigned_upload: true, frontend_direct_provider_resumable_upload: false },
	config_schema_version: 1,
	credential_schema_version: 1,
	fields: [
		{
			help_key: null,
			kind: "text",
			label_key: "endpoint",
			name: "endpoint",
			placeholder: "https://s3.example.com",
			required: true,
			secret: false,
		},
		{
			help_key: null,
			kind: "text",
			label_key: "bucket",
			name: "bucket",
			placeholder: null,
			required: true,
			secret: false,
		},
		{
			help_key: null,
			kind: "text",
			label_key: "access_key",
			name: "access_key",
			placeholder: null,
			required: true,
			secret: false,
		},
		{
			help_key: null,
			kind: "secret",
			label_key: "secret_key",
			name: "secret_key",
			placeholder: null,
			required: true,
			secret: true,
		},
		{
			help_key: "remote_node_ingress_profile_s3_path_hint",
			kind: "text",
			label_key: "base_path",
			name: "base_path",
			placeholder: "prefix",
			required: false,
			secret: false,
		},
		{
			help_key: "remote_node_ingress_profile_default_hint",
			kind: "boolean",
			label_key: "remote_node_ingress_profile_default_toggle",
			name: "is_default",
			placeholder: null,
			required: false,
			secret: false,
		},
	],
	actions: [],
	promotions: [],
	related_issues: [],
};

const defaultDriverDescriptors = [localDriverDescriptor, s3DriverDescriptor];

function renderSection({
	allowCreate = false,
	createLabelKey,
	driverDescriptors = defaultDriverDescriptors,
	errorMessage = null,
	loading = false,
	onCreateTarget = vi.fn().mockResolvedValue(undefined),
	onDeleteTarget = vi.fn().mockResolvedValue(undefined),
	onUpdateTarget = vi.fn().mockResolvedValue(undefined),
	readOnly = false,
	targets = [] as RemoteStorageTargetInfo[],
} = {}) {
	render(
		<RemoteNodeRemoteStorageTargetSection
			allowCreate={allowCreate}
			createLabelKey={createLabelKey}
			driverDescriptors={driverDescriptors}
			errorMessage={errorMessage}
			loading={loading}
			onCreateTarget={onCreateTarget}
			onDeleteTarget={onDeleteTarget}
			onUpdateTarget={onUpdateTarget}
			readOnly={readOnly}
			targets={targets}
		/>,
	);
	return {
		onCreateTarget,
		onDeleteTarget,
		onUpdateTarget,
	};
}

describe("RemoteNodeRemoteStorageTargetSection", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("shows loading, empty and error states", () => {
		const { rerender } = render(
			<RemoteNodeRemoteStorageTargetSection
				driverDescriptors={defaultDriverDescriptors}
				errorMessage={null}
				loading
				onCreateTarget={vi.fn()}
				onDeleteTarget={vi.fn()}
				onUpdateTarget={vi.fn()}
				targets={[]}
			/>,
		);

		expect(screen.getByText("core:loading")).toBeInTheDocument();

		rerender(
			<RemoteNodeRemoteStorageTargetSection
				driverDescriptors={defaultDriverDescriptors}
				errorMessage={null}
				loading={false}
				onCreateTarget={vi.fn()}
				onDeleteTarget={vi.fn()}
				onUpdateTarget={vi.fn()}
				targets={[]}
			/>,
		);

		expect(
			screen.getByText("remote_node_ingress_profiles_empty"),
		).toBeInTheDocument();

		rerender(
			<RemoteNodeRemoteStorageTargetSection
				driverDescriptors={defaultDriverDescriptors}
				errorMessage="cannot reach node"
				loading={false}
				onCreateTarget={vi.fn()}
				onDeleteTarget={vi.fn()}
				onUpdateTarget={vi.fn()}
				targets={[]}
			/>,
		);

		expect(screen.getByText("cannot reach node")).toBeInTheDocument();
		expect(
			screen.getByRole("button", {
				name: /remote_node_ingress_profiles_create/,
			}),
		).toBeDisabled();
	});

	it("renders existing targets behind a collapsed read-only disclosure", async () => {
		const user = userEvent.setup();
		renderSection({
			readOnly: true,
			targets: [profile()],
		});

		const toggle = screen.getByRole("button", {
			name: "policy_remote_storage_targets_show",
		});
		expect(toggle).toHaveAttribute("aria-expanded", "false");
		expect(screen.queryByText("Local ingress")).not.toBeInTheDocument();

		await user.click(toggle);

		expect(
			screen.getByRole("button", {
				name: "policy_remote_storage_targets_hide",
			}),
		).toHaveAttribute("aria-expanded", "true");
		expect(screen.getByText("Local ingress")).toBeInTheDocument();
		expect(screen.queryByText("local-default")).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", {
				name: "remote_node_ingress_profiles_create",
			}),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "core:edit" }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "core:delete" }),
		).not.toBeInTheDocument();
	});

	it("allows quick creation in a read-only target list without exposing management actions", async () => {
		const user = userEvent.setup();
		const { onCreateTarget } = renderSection({
			allowCreate: true,
			createLabelKey: "policy_remote_storage_targets_quick_create",
			readOnly: true,
			targets: [profile()],
		});

		expect(screen.queryByText("Local ingress")).not.toBeInTheDocument();
		await user.click(
			screen.getByRole("button", {
				name: "policy_remote_storage_targets_quick_create",
			}),
		);

		expect(screen.getByText("Local ingress")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "core:edit" }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "core:delete" }),
		).not.toBeInTheDocument();
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Policy quick target" },
		});
		fireEvent.change(screen.getByLabelText("base_path"), {
			target: { value: "policy/incoming" },
		});
		fireEvent.click(screen.getByRole("button", { name: /core:create/ }));

		await waitFor(() => {
			expect(onCreateTarget).toHaveBeenCalledWith(
				expect.objectContaining({
					connector_config: expect.objectContaining({ values: expect.objectContaining({ base_path: "policy/incoming" }) }),
					driver_type: "connector",
					is_default: false,
					name: "Policy quick target",
				}),
			);
		});
	});

	it("creates the first local profile as the default", async () => {
		const { onCreateTarget } = renderSection();

		const createButton = screen.getByRole("button", {
			name: /remote_node_ingress_profiles_create/,
		});
		fireEvent.click(createButton);
		expect(
			screen.getByLabelText("remote_node_ingress_profile_default_toggle"),
		).toBeChecked();
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: " Local upload " },
		});
		fireEvent.change(screen.getByLabelText("base_path"), {
			target: { value: "teams/incoming" },
		});
		fireEvent.click(screen.getByRole("button", { name: /core:create/ }));

		await waitFor(() => {
				expect(onCreateTarget).toHaveBeenCalledWith(expect.objectContaining({
					connector_config: expect.objectContaining({ connector_id: "asterdrive.storage.local", values: expect.objectContaining({ base_path: "teams/incoming" }) }),
					driver_type: "connector",
					is_default: true,
					name: "Local upload",
				}));
		});
		expect(
			screen.queryByText("remote_node_ingress_profile_form_create_title"),
		).not.toBeInTheDocument();
	});

	it("does not create targets when no supported driver descriptor is returned", () => {
		renderSection({ driverDescriptors: [] });

		expect(
			screen.getByRole("button", {
				name: /remote_node_ingress_profiles_create/,
			}),
		).toBeDisabled();
	});

	it("keeps the create draft closed when no create handler is available", () => {
		render(
			<RemoteNodeRemoteStorageTargetSection
				driverDescriptors={defaultDriverDescriptors}
				errorMessage={null}
				loading={false}
				targets={[]}
			/>,
		);

		const createButton = screen.getByRole("button", {
			name: /remote_node_ingress_profiles_create/,
		});
		expect(createButton).toBeDisabled();
		fireEvent.click(createButton);

		expect(
			screen.queryByText("remote_node_ingress_profile_form_create_title"),
		).not.toBeInTheDocument();
	});

	it("validates S3 credentials on create and submits normalized fields", async () => {
		const { onCreateTarget } = renderSection({ targets: [profile()] });

		fireEvent.click(
			screen.getByRole("button", {
				name: /remote_node_ingress_profiles_create/,
			}),
		);
		fireEvent.change(screen.getByLabelText("select:asterdrive.storage.local"), {
			target: { value: "asterdrive.storage.s3" },
		});

		expect(screen.getByRole("button", { name: /core:create/ })).toBeDisabled();
		expect(
			screen.getByText("remote_node_ingress_profile_name_required"),
		).toBeInTheDocument();
		expect(
			screen.getByText("remote_node_ingress_profile_endpoint_required"),
		).toBeInTheDocument();
		expect(
			screen.getByText("remote_node_ingress_profile_access_key_required"),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "S3 upload" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://s3.example.test/raw-bucket" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: " raw-bucket " },
		});
		fireEvent.change(screen.getByLabelText("access_key"), {
			target: { value: " access " },
		});
		fireEvent.change(screen.getByLabelText("secret_key"), {
			target: { value: " secret " },
		});
		fireEvent.click(screen.getByRole("button", { name: /core:create/ }));

		await waitFor(() => {
			expect(onCreateTarget).toHaveBeenCalledWith(
				expect.objectContaining({
					connector_config: expect.objectContaining({ connector_id: "asterdrive.storage.s3", values: expect.objectContaining({ bucket: "raw-bucket", endpoint: "https://s3.example.test/raw-bucket" }) }),
					credential: { access_key: "access", secret_key: "secret" },
					driver_type: "connector",
					name: "S3 upload",
				}),
			);
		});
	});

	it("edits existing S3 targets while requiring access key but preserving secret", async () => {
		const existing = profile({
			base_path: "prefix",
			bucket: "bucket-a",
			driver_type: "s3",
			connector_id: "asterdrive.storage.s3",
			endpoint: "https://s3.example.com",
			is_default: true,
			name: "S3 ingress",
			target_key: "s3-default",
		});
		const { onUpdateTarget } = renderSection({ targets: [existing] });

		fireEvent.click(screen.getByRole("button", { name: "core:edit" }));
		expect(
			screen.getByLabelText("remote_node_ingress_profile_default_toggle"),
		).toBeDisabled();
		expect(screen.getByRole("button", { name: /save_changes/ })).toBeDisabled();
		expect(
			screen.getByText("remote_node_ingress_profile_access_key_required"),
		).toBeInTheDocument();
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "S3 renamed" },
		});
		fireEvent.change(screen.getByLabelText("access_key"), {
			target: { value: "rotated-access" },
		});
		fireEvent.change(screen.getByLabelText("base_path"), {
			target: { value: "next-prefix" },
		});
		fireEvent.click(screen.getByRole("button", { name: /save_changes/ }));

		await waitFor(() => {
				expect(onUpdateTarget).toHaveBeenCalledWith("s3-default", expect.objectContaining({
					connector_config: expect.objectContaining({ connector_id: "asterdrive.storage.s3", values: expect.objectContaining({ base_path: "next-prefix" }) }),
					credential: { access_key: "rotated-access" },
					is_default: true,
					name: "S3 renamed",
				}));
		});
	});

	it("confirms deletion and resets an edited draft when the profile disappears", async () => {
		const existing = profile();
		const onDeleteTarget = vi.fn().mockResolvedValue(undefined);
		const { rerender } = render(
			<RemoteNodeRemoteStorageTargetSection
				driverDescriptors={defaultDriverDescriptors}
				errorMessage={null}
				loading={false}
				onCreateTarget={vi.fn()}
				onDeleteTarget={onDeleteTarget}
				onUpdateTarget={vi.fn()}
				targets={[existing]}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "core:edit" }));
		expect(
			screen.getByText("remote_node_ingress_profile_form_edit_title"),
		).toBeInTheDocument();

		rerender(
			<RemoteNodeRemoteStorageTargetSection
				driverDescriptors={defaultDriverDescriptors}
				errorMessage={null}
				loading={false}
				onCreateTarget={vi.fn()}
				onDeleteTarget={onDeleteTarget}
				onUpdateTarget={vi.fn()}
				targets={[]}
			/>,
		);

		expect(
			screen.queryByText("remote_node_ingress_profile_form_edit_title"),
		).not.toBeInTheDocument();

		rerender(
			<RemoteNodeRemoteStorageTargetSection
				driverDescriptors={defaultDriverDescriptors}
				errorMessage={null}
				loading={false}
				onCreateTarget={vi.fn()}
				onDeleteTarget={onDeleteTarget}
				onUpdateTarget={vi.fn()}
				targets={[existing]}
			/>,
		);
		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));
		const deleteNotice = screen.getByText(
			"remote_node_ingress_profile_delete_title:Local ingress",
		).parentElement;
		expect(deleteNotice).toHaveClass(
			"animate-in",
			"motion-reduce:animate-none",
		);
		fireEvent.click(
			within(deleteNotice?.parentElement ?? document.body).getByRole("button", {
				name: "core:cancel",
			}),
		);
		expect(onDeleteTarget).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));
		fireEvent.click(screen.getAllByRole("button", { name: "core:delete" })[0]);

		await waitFor(() => {
			expect(onDeleteTarget).toHaveBeenCalledWith(existing);
		});
	});

	it("ignores delete confirmation when no delete handler is available", () => {
		const existing = profile();
		render(
			<RemoteNodeRemoteStorageTargetSection
				driverDescriptors={defaultDriverDescriptors}
				errorMessage={null}
				loading={false}
				onCreateTarget={vi.fn()}
				onUpdateTarget={vi.fn()}
				targets={[existing]}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));
		fireEvent.click(screen.getAllByRole("button", { name: "core:delete" })[0]);

		expect(
			screen.getByText(
				"remote_node_ingress_profile_delete_title:Local ingress",
			),
		).toBeInTheDocument();
	});
});

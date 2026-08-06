import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type {
	RemoteNodeInfo,
	RemoteStorageTargetInfo,
	StorageConnectorActionDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import {
	StorageConnectorActionsPanel,
	type StorageConnectorActionValues,
} from "./StorageConnectorActionsPanel";
import type { Translate } from "./StoragePolicyFieldTypes";

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		disabled,
		items = [],
		value,
		onValueChange,
	}: {
		children: ReactNode;
		disabled?: boolean;
		items?: Array<{ label: string; value: string }>;
		value?: string | null;
		onValueChange?: (value: string | null) => void;
	}) => (
		<div>
			{children}
			<select
				aria-label="descriptor-select"
				disabled={disabled}
				value={value ?? ""}
				onChange={(event) => onValueChange?.(event.target.value || null)}
			>
				<option value="" />
				{items.map((item) => (
					<option key={item.value} value={item.value}>
						{item.label}
					</option>
				))}
			</select>
		</div>
	),
	SelectContent: () => null,
	SelectItem: () => null,
	SelectTrigger: () => null,
	SelectValue: () => null,
}));

const labels: Record<string, string> = {
	"action.first": "First action",
	"action.first.desc": "Inspect the first remote namespace.",
	"action.second": "Second action",
	"action.second.desc": "Inspect the second remote namespace.",
	"field.enabled": "Enabled",
	"field.mode": "Mode",
	"field.path": "Path",
	"field.retries": "Retries",
	"field.token": "Token",
	"mode.check": "Check",
	"mode.repair": "Repair",
	policy_connector_action_confirm: "Confirm action",
	policy_connector_action_confirm_desc: "Remote state may change.",
	policy_connector_action_confirm_title: "Run {{action}}?",
	"core:cancel": "Cancel",
};

const t: Translate = (key, values) => {
	const label = labels[key] ?? key;
	return Object.entries(values ?? {}).reduce(
		(current, [name, value]) => current.replace(`{{${name}}}`, String(value)),
		label,
	);
};

function field(
	name: string,
	kind: StorageConnectorFieldDescriptor["kind"],
	overrides: Partial<StorageConnectorFieldDescriptor> = {},
): StorageConnectorFieldDescriptor {
	return {
		kind,
		label_key: `field.${name}`,
		name,
		required: false,
		scope: "action_input",
		secret: kind === "secret",
		...overrides,
	};
}

function action(
	actionId: string,
	overrides: Partial<StorageConnectorActionDescriptor> = {},
): StorageConnectorActionDescriptor {
	const suffix = actionId.endsWith("second") ? "second" : "first";
	return {
		action_id: actionId,
		description_key: `action.${suffix}.desc`,
		kind: "custom",
		label_key: `action.${suffix}`,
		mutates_remote_state: false,
		requires_authorization: false,
		requires_confirmation: false,
		requires_saved_policy: false,
		...overrides,
	};
}

function renderPanel({
	actions,
	confirmActionId = null,
	submittingActionId = null,
	values = {},
	remoteNodes = [],
	remoteStorageTargets = [],
}: {
	actions: StorageConnectorActionDescriptor[];
	confirmActionId?: string | null;
	submittingActionId?: string | null;
	values?: StorageConnectorActionValues;
	remoteNodes?: RemoteNodeInfo[];
	remoteStorageTargets?: RemoteStorageTargetInfo[];
}) {
	const callbacks = {
		onCancel: vi.fn(),
		onConfirm: vi.fn(),
		onRequest: vi.fn(),
		onValueChange: vi.fn(),
	};
	const view = render(
		<StorageConnectorActionsPanel
			actions={actions}
			remoteNodes={remoteNodes}
			remoteStorageTargets={remoteStorageTargets}
			confirmActionId={confirmActionId}
			submittingActionId={submittingActionId}
			t={t}
			values={values}
			{...callbacks}
		/>,
	);
	return { ...callbacks, ...view };
}

function remoteNode(id: number, name: string): RemoteNodeInfo {
	return {
		base_url: `https://node-${id}.example.com`,
		capabilities: {},
		created_at: "2026-08-05T00:00:00Z",
		enrollment_status: "completed",
		id,
		is_enabled: true,
		last_checked_at: null,
		last_error: "",
		name,
		transport_mode: "direct",
		tunnel: { last_error: "", last_seen_at: null, status: "offline" },
		updated_at: "2026-08-05T00:00:00Z",
	};
}

function remoteTarget(targetKey: string): RemoteStorageTargetInfo {
	return {
		applied_revision: 1,
		base_path: "",
		bucket: "",
		created_at: "2026-08-05T00:00:00Z",
		desired_revision: 1,
		driver_type: "local",
		endpoint: "",
		is_default: true,
		last_error: "",
		name: "Archive",
		target_key: targetKey,
		updated_at: "2026-08-05T00:00:00Z",
	};
}

describe("StorageConnectorActionsPanel", () => {
	it("renders nothing when the connector declares no custom actions", () => {
		const { container } = renderPanel({ actions: [] });

		expect(container).toBeEmptyDOMElement();
	});

	it("renders multiple actions and keeps same-named field values isolated by action id", () => {
		renderPanel({
			actions: [
				action("plugin.first", { fields: [field("path", "text")] }),
				action("plugin.second", { fields: [field("path", "text")] }),
			],
			values: {
				"plugin.first": { path: "/first" },
				"plugin.second": { path: "/second" },
			},
		});

		expect(screen.getByRole("button", { name: "First action" })).toBeVisible();
		expect(screen.getByRole("button", { name: "Second action" })).toBeVisible();
		expect(
			screen.getByText("Inspect the first remote namespace."),
		).toBeVisible();
		expect(
			screen.getByText("Inspect the second remote namespace."),
		).toBeVisible();
		expect(screen.getByDisplayValue("/first")).toHaveAttribute(
			"id",
			"storage-action-plugin.first-path",
		);
		expect(screen.getByDisplayValue("/second")).toHaveAttribute(
			"id",
			"storage-action-plugin.second-path",
		);
	});

	it("renders every scalar field kind from the action schema and reports typed changes", () => {
		const { onValueChange } = renderPanel({
			actions: [
				action("plugin.first", {
					fields: [
						field("path", "text", { trim_on_blur: true }),
						field("token", "secret", { help_key: "field.token.help" }),
						field("enabled", "boolean", { default_value: true }),
						field("retries", "number", { default_value: 3 }),
						field("mode", "select", {
							default_value: "mode.check",
							select: {
								options: [
									{ label_key: "mode.check", value: "mode.check" },
									{ label_key: "mode.repair", value: "mode.repair" },
								],
								value_kind: "string",
							},
						}),
					],
				}),
			],
			values: { "plugin.first": { path: "  /uploads  " } },
		});

		const path = screen.getByLabelText("Path");
		const token = screen.getByLabelText("Token");
		const retries = screen.getByLabelText("Retries");
		expect(token).toHaveAttribute("type", "password");
		expect(retries).toHaveValue(3);
		expect(screen.getByRole("switch")).toBeChecked();
		expect(screen.getByRole("combobox")).toHaveValue("mode.check");

		fireEvent.blur(path, { target: { value: "  /uploads  " } });
		fireEvent.change(token, { target: { value: "TOKEN" } });
		fireEvent.click(screen.getByRole("switch"));
		fireEvent.change(retries, { target: { value: "5" } });
		fireEvent.change(screen.getByRole("combobox"), {
			target: { value: "mode.repair" },
		});
		fireEvent.blur(token, { target: { value: "TOKEN" } });
		fireEvent.blur(retries, { target: { value: "5" } });
		fireEvent.change(retries, { target: { value: "" } });

		expect(onValueChange).toHaveBeenCalledWith(
			"plugin.first",
			"path",
			"/uploads",
		);
		expect(onValueChange).toHaveBeenCalledWith(
			"plugin.first",
			"token",
			"TOKEN",
		);
		expect(onValueChange).toHaveBeenCalledWith(
			"plugin.first",
			"enabled",
			false,
		);
		expect(onValueChange).toHaveBeenCalledWith("plugin.first", "retries", 5);
		expect(onValueChange).toHaveBeenCalledWith(
			"plugin.first",
			"mode",
			"mode.repair",
		);
		expect(onValueChange).toHaveBeenCalledWith(
			"plugin.first",
			"retries",
			undefined,
		);
		expect(screen.getByText("field.token.help")).toBeVisible();
	});

	it("shows confirmation only for the selected action and routes confirm and cancel", () => {
		const { onCancel, onConfirm, onRequest } = renderPanel({
			actions: [action("plugin.first"), action("plugin.second")],
			confirmActionId: "plugin.second",
		});

		expect(screen.getByText("Run Second action?")).toBeVisible();
		expect(screen.queryByText("Run First action?")).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "First action" }));
		fireEvent.click(screen.getByRole("button", { name: "Confirm action" }));
		fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

		expect(onRequest).toHaveBeenCalledWith("plugin.first");
		expect(onConfirm).toHaveBeenCalledWith("plugin.second");
		expect(onCancel).toHaveBeenCalledOnce();
	});

	it("disables only the submitting action and tolerates a select schema without options", () => {
		renderPanel({
			actions: [
				action("plugin.first", { fields: [field("mode", "select")] }),
				action("plugin.second"),
			],
			submittingActionId: "plugin.first",
		});

		expect(screen.getByRole("button", { name: "First action" })).toBeDisabled();
		expect(screen.getByRole("button", { name: "Second action" })).toBeEnabled();
		expect(screen.getByRole("combobox")).toHaveValue("");
	});

	it("renders dynamic remote lookups and disables dependent selects until their source is chosen", () => {
		const { onValueChange } = renderPanel({
			actions: [
				action("plugin.first", {
					fields: [
						field("node", "select", {
							select: { data_source: "remote_nodes", value_kind: "integer" },
						}),
						field("target", "select", {
							select: {
								data_source: "remote_storage_targets",
								depends_on: "node",
								value_kind: "string",
							},
						}),
					],
				}),
			],
			remoteNodes: [remoteNode(7, "Node seven")],
			remoteStorageTargets: [{ ...remoteTarget("archive"), name: "" }],
			values: { "plugin.first": { target: "archive" } },
		});

		const selects = screen.getAllByRole("combobox");
		expect(selects[0]).toHaveTextContent("Node seven");
		expect(selects[1]).toHaveTextContent("archive");
		expect(selects[1]).toBeDisabled();
		fireEvent.change(selects[0], { target: { value: "7" } });
		fireEvent.change(selects[0], { target: { value: "" } });
		expect(onValueChange).toHaveBeenCalledWith("plugin.first", "node", 7);
		expect(onValueChange).toHaveBeenCalledWith(
			"plugin.first",
			"target",
			undefined,
		);
		expect(onValueChange).toHaveBeenCalledWith(
			"plugin.first",
			"node",
			undefined,
		);
	});

	it("uses a descriptor default when resolving an action select dependency", () => {
		renderPanel({
			actions: [
				action("plugin.first", {
					fields: [
						field("node", "select", {
							default_value: 7,
							select: { data_source: "remote_nodes", value_kind: "integer" },
						}),
						field("target", "select", {
							select: {
								data_source: "remote_storage_targets",
								depends_on: "node",
								value_kind: "string",
							},
						}),
					],
				}),
			],
			remoteNodes: [remoteNode(7, "Node seven")],
		});

		const selects = screen.getAllByRole("combobox");
		expect(selects[0]).toHaveValue("7");
		expect(selects[1]).toBeEnabled();
	});
});

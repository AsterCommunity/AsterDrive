import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import { emptyForm, type PolicyFormData } from "./formTypes";
import { StorageConnectorFieldsPanel } from "./StorageConnectorFieldsPanel";
import type { Translate } from "./StoragePolicyFieldTypes";

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		disabled,
		items = [],
		onValueChange,
		value,
	}: {
		children: ReactNode;
		disabled?: boolean;
		items?: Array<{ label: string; value: string }>;
		onValueChange?: (value: string | null) => void;
		value?: string | null;
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
	base_path: "Base path",
	boolean_field: "Boolean field",
	empty_select: "Empty select",
	mode: "Mode",
	mode_direct: "Direct",
	mode_direct_desc: "Provider direct transfer",
	mode_relay: "Relay",
	mode_relay_desc: "Relay through AsterDrive",
	number_field: "Number field",
	policy_connector_field_required: "{{field}} is required",
	remote_node_id: "Remote node",
	remote_storage_target_key: "Remote target",
	secret_field: "Secret field",
	text_field: "Text field",
	text_field_help: "Text field help",
	text_field_required: "Text field is connector-required",
};

const t: Translate = (key, values) =>
	Object.entries(values ?? {}).reduce(
		(text, [name, value]) => text.replace(`{{${name}}}`, String(value)),
		labels[key] ?? key,
	);

function field(
	name: string,
	kind: StorageConnectorFieldDescriptor["kind"],
	overrides: Partial<StorageConnectorFieldDescriptor> = {},
): StorageConnectorFieldDescriptor {
	return {
		kind,
		label_key: name,
		name,
		required: false,
		scope: "connector_config",
		secret: kind === "secret",
		...overrides,
	};
}

function descriptor(fields: StorageConnectorFieldDescriptor[]) {
	return { fields } as StorageConnectorDescriptor;
}

function renderPanel({
	fields,
	form = emptyForm,
	mode = "create",
	showRequiredErrors = false,
	descriptorValue = descriptor(fields),
	declaredFields,
}: {
	fields: StorageConnectorFieldDescriptor[];
	form?: PolicyFormData;
	mode?: "create" | "edit";
	showRequiredErrors?: boolean;
	descriptorValue?: StorageConnectorDescriptor | null;
	declaredFields?: StorageConnectorFieldDescriptor[];
}) {
	const onFieldChange = vi.fn();
	const view = render(
		<StorageConnectorFieldsPanel
			descriptor={descriptorValue}
			fields={declaredFields}
			form={form}
			mode={mode}
			onFieldChange={onFieldChange}
			remoteNodes={
				[
					{
						base_url: "https://node.example.test",
						created_at: "2026-01-01T00:00:00Z",
						id: 7,
						is_enabled: true,
						name: "Node seven",
						node_id: "node-seven",
						status: "online",
						transport_mode: "direct",
						updated_at: "2026-01-01T00:00:00Z",
					},
				] as never
			}
			remoteStorageTargets={[{ name: "", target_key: "archive" }] as never}
			showRequiredErrors={showRequiredErrors}
			t={t}
		/>,
	);
	return { ...view, onFieldChange };
}

describe("StorageConnectorFieldsPanel", () => {
	it("renders nothing when the descriptor has no visible fields", () => {
		const { container } = renderPanel({ fields: [] });
		expect(container).toBeEmptyDOMElement();

		const descriptorless = renderPanel({ fields: [], descriptorValue: null });
		expect(descriptorless.container).toBeEmptyDOMElement();
	});

	it("renders explicitly declared fields without a descriptor", () => {
		const explicitField = field("text_field", "text");
		const { onFieldChange } = renderPanel({
			fields: [],
			declaredFields: [explicitField],
			descriptorValue: null,
		});

		fireEvent.change(screen.getByLabelText("Text field"), {
			target: { value: "standalone" },
		});
		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {
			text_field: "standalone",
		});
	});

	it("lets a single field fill its parent and only splits multiple fields into columns", () => {
		const single = renderPanel({ fields: [field("base_path", "text")] });
		expect(single.container.firstElementChild).toHaveClass("grid", "gap-4");
		expect(single.container.firstElementChild).not.toHaveClass(
			"md:grid-cols-2",
		);
		single.unmount();

		const multiple = renderPanel({
			fields: [field("base_path", "text"), field("text_field", "text")],
		});
		expect(multiple.container.firstElementChild).toHaveClass("md:grid-cols-2");
	});

	it("renders scalar controls with defaults, validation attributes, and required feedback", () => {
		renderPanel({
			fields: [
				field("text_field", "text", {
					help_key: "text_field_help",
					required: true,
					required_message_key: "text_field_required",
					trim_on_blur: true,
					validation: { max_length: 12 },
				}),
				field("base_path", "text", { required: true }),
				field("secret_field", "secret", { scope: "static_credential" }),
				field("number_field", "number", {
					default_value: 5,
					validation: { max_integer: 10, min_integer: 1 },
				}),
				field("boolean_field", "boolean", { default_value: true }),
			],
			showRequiredErrors: true,
		});

		expect(screen.getByLabelText("Text field")).toHaveAttribute(
			"maxlength",
			"12",
		);
		expect(screen.getByText("Text field is connector-required")).toBeVisible();
		expect(screen.getByText("Base path is required")).toBeVisible();
		expect(screen.getByText("Text field help")).toBeVisible();
		expect(screen.getByLabelText("Secret field")).toHaveAttribute(
			"type",
			"password",
		);
		expect(screen.getByLabelText("Secret field")).toHaveValue("");
		expect(screen.getByLabelText("Number field")).toHaveValue(5);
		expect(screen.getByLabelText("Number field")).toHaveAttribute("min", "1");
		expect(screen.getByLabelText("Number field")).toHaveAttribute("max", "10");
		expect(screen.getByRole("switch")).toBeChecked();
	});

	it("normalizes text and number changes while keeping credential and config channels separate", () => {
		const form: PolicyFormData = {
			...emptyForm,
			connector_config_values: { existing: "config" },
			credential_values: { existing_secret: "keep" },
		};
		const { onFieldChange } = renderPanel({
			fields: [
				field("text_field", "text", { trim_on_blur: true }),
				field("number_field", "number"),
				field("secret_field", "secret", { scope: "static_credential" }),
			],
			form,
		});

		fireEvent.blur(screen.getByLabelText("Text field"), {
			target: { value: "  value  " },
		});
		fireEvent.change(screen.getByLabelText("Number field"), {
			target: { value: "8" },
		});
		fireEvent.change(screen.getByLabelText("Secret field"), {
			target: { value: "TOKEN" },
		});

		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {
			existing: "config",
			text_field: "value",
		});
		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {
			existing: "config",
			number_field: 8,
		});
		expect(onFieldChange).toHaveBeenCalledWith("credential_values", {
			existing_secret: "keep",
			secret_field: "TOKEN",
		});
	});

	it("updates booleans and recursively clears credential dependencies", () => {
		const fields = [
			field("boolean_field", "boolean"),
			field("secret_field", "secret", { scope: "static_credential" }),
			field("dependent_secret", "secret", {
				scope: "static_credential",
				select: { depends_on: "secret_field", value_kind: "string" },
			}),
		];
		const { onFieldChange } = renderPanel({
			fields,
			form: {
				...emptyForm,
				credential_values: {
					dependent_secret: "stale",
					secret_field: "old",
				},
			},
		});

		fireEvent.click(screen.getByRole("switch"));
		fireEvent.change(screen.getByLabelText("Secret field"), {
			target: { value: "new" },
		});

		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {
			boolean_field: true,
		});
		expect(onFieldChange).toHaveBeenCalledWith("credential_values", {
			secret_field: "new",
		});
	});

	it("removes a cleared optional number so the descriptor default can be restored", () => {
		const { onFieldChange } = renderPanel({
			fields: [field("number_field", "number", { default_value: 30 })],
			form: {
				...emptyForm,
				connector_config_values: { number_field: 15 },
			},
		});

		fireEvent.change(screen.getByLabelText("Number field"), {
			target: { value: "" },
		});
		fireEvent.blur(screen.getByLabelText("Number field"));

		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {});
	});

	it("renders defensive scalar fallbacks and stringifies credential booleans", () => {
		const textView = renderPanel({
			fields: [field("text_field", "text")],
			form: {
				...emptyForm,
				connector_config_values: { text_field: true },
			},
		});
		expect(screen.getByLabelText("Text field")).toHaveValue("");
		textView.unmount();

		const credentialBoolean = field("credential_boolean", "boolean", {
			scope: "static_credential",
		});
		const credentialSelect = field("credential_select", "select", {
			scope: "static_credential",
			select: { options: [], value_kind: "string" },
		});
		const { onFieldChange } = renderPanel({
			fields: [credentialBoolean, credentialSelect],
			form: {
				...emptyForm,
				credential_values: { credential_select: "old" },
			},
		});
		fireEvent.click(screen.getByRole("switch"));
		fireEvent.change(screen.getByRole("combobox"), { target: { value: "" } });
		expect(onFieldChange).toHaveBeenCalledWith("credential_values", {
			credential_boolean: "true",
			credential_select: "old",
		});
		expect(onFieldChange).toHaveBeenCalledWith("credential_values", {
			credential_select: "",
		});
	});

	it("does not report a required field missing when its descriptor supplies a default", () => {
		renderPanel({
			fields: [
				field("number_field", "number", {
					default_value: 5,
					required: true,
				}),
			],
			showRequiredErrors: true,
		});

		expect(screen.getByLabelText("Number field")).not.toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(
			screen.queryByText("Number field is required"),
		).not.toBeInTheDocument();
	});

	it("keeps stable field ids and disambiguates duplicate names across scopes", () => {
		renderPanel({
			fields: [
				field("base_path", "text"),
				field("text_field", "text", { scope: "connector_config" }),
				field("text_field", "secret", { scope: "static_credential" }),
			],
		});

		expect(screen.getByLabelText("Base path")).toHaveAttribute(
			"id",
			"base_path",
		);
		const duplicateFields = screen.getAllByLabelText("Text field");
		expect(duplicateFields[0]).toHaveAttribute(
			"id",
			"storage-connector-connector_config-text_field",
		);
		expect(duplicateFields[1]).toHaveAttribute(
			"id",
			"storage-connector-static_credential-text_field",
		);
	});

	it("resolves an empty optional text field to its connector-owned default on blur", () => {
		const { onFieldChange } = renderPanel({
			fields: [
				field("base_path", "text", {
					default_mode: "missing_or_empty_text",
					default_value: "./data/uploads",
					trim_on_blur: true,
				}),
			],
			form: {
				...emptyForm,
				connector_config_values: { base_path: "" },
			},
		});

		fireEvent.blur(screen.getByLabelText("Base path"), {
			target: { value: "   " },
		});

		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {
			base_path: "./data/uploads",
		});
	});

	it("uses connector-owned static labels and dynamic catalogs with typed values", () => {
		const fields = [
			field("mode", "select", {
				default_value: "relay",
				select: {
					options: [
						{
							description_key: "mode_relay_desc",
							label_key: "mode_relay",
							value: "relay",
						},
						{
							description_key: "mode_direct_desc",
							label_key: "mode_direct",
							value: "direct",
						},
					],
					value_kind: "string",
				},
			}),
			field("remote_node_id", "select", {
				select: { data_source: "remote_nodes", value_kind: "integer" },
			}),
			field("remote_storage_target_key", "select", {
				select: {
					data_source: "remote_storage_targets",
					depends_on: "remote_node_id",
					value_kind: "string",
				},
			}),
		];
		const { onFieldChange } = renderPanel({
			fields,
			form: {
				...emptyForm,
				connector_config_values: {
					remote_storage_target_key: "archive",
				},
			},
		});
		const selects = screen.getAllByRole("combobox");

		expect(selects[0]).toHaveValue("relay");
		expect(screen.getByText("Relay through AsterDrive")).toBeVisible();
		expect(screen.getByRole("option", { name: "Direct" })).toBeVisible();
		expect(screen.getByRole("option", { name: "Node seven" })).toBeVisible();
		expect(selects[2]).toBeDisabled();
		fireEvent.change(selects[0], { target: { value: "direct" } });
		fireEvent.change(selects[1], { target: { value: "7" } });

		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {
			mode: "direct",
			remote_storage_target_key: "archive",
		});
		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {
			remote_node_id: 7,
		});
	});

	it("disables a select whose declared dependency is absent", () => {
		renderPanel({
			fields: [
				field("mode", "select", {
					select: { depends_on: "missing", value_kind: "string" },
				}),
			],
		});

		expect(screen.getByRole("combobox")).toBeDisabled();
	});

	it("uses the connector placeholder on create and the keep-existing hint for edit credentials", () => {
		const credentialField = field("secret_field", "secret", {
			placeholder: "create-secret-placeholder",
			scope: "static_credential",
		});
		const createView = renderPanel({ fields: [credentialField] });
		expect(screen.getByLabelText("Secret field")).toHaveAttribute(
			"placeholder",
			"create-secret-placeholder",
		);
		createView.unmount();

		renderPanel({ fields: [credentialField], mode: "edit" });
		expect(screen.getByLabelText("Secret field")).toHaveAttribute(
			"placeholder",
			"policy_editor_credentials_keep_placeholder",
		);
	});

	it("recursively clears dependent values and tolerates an empty static option list", () => {
		const fields = [
			field("remote_node_id", "select", {
				select: { data_source: "remote_nodes", value_kind: "integer" },
			}),
			field("remote_storage_target_key", "select", {
				select: {
					data_source: "remote_storage_targets",
					depends_on: "remote_node_id",
					value_kind: "string",
				},
			}),
			field("empty_select", "select", {
				select: {
					depends_on: "remote_storage_target_key",
					options: [],
					value_kind: "string",
				},
			}),
		];
		const { onFieldChange } = renderPanel({
			fields,
			form: {
				...emptyForm,
				connector_config_values: {
					empty_select: "stale-child",
					remote_node_id: 3,
					remote_storage_target_key: "stale-target",
				},
			},
		});

		fireEvent.change(screen.getAllByRole("combobox")[0], {
			target: { value: "7" },
		});

		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {
			remote_node_id: 7,
		});
		expect(screen.getAllByRole("combobox")[2]).toBeEnabled();
		expect(
			screen.getAllByRole("combobox")[2].querySelectorAll("option"),
		).toHaveLength(1);

		fireEvent.change(screen.getAllByRole("combobox")[0], {
			target: { value: "" },
		});
		expect(onFieldChange).toHaveBeenCalledWith("connector_config_values", {
			remote_node_id: null,
		});
	});
});

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DriverType, StorageConnectorDescriptor } from "@/types/api";
import { emptyForm } from "./formTypes";
import type { Translate } from "./StoragePolicyFieldTypes";
import { ObjectStorageConnectionFields } from "./StoragePolicyObjectStorageFields";

const labels: Record<string, string> = {
	access_key: "Access key",
	azure_blob_account_key: "Account key",
	azure_blob_account_name: "Account name",
	azure_blob_endpoint_hint: "Azure Blob endpoint hint",
	bucket: "Bucket",
	cos_endpoint_hint: "COS endpoint hint",
	s3_endpoint_hint: "S3 endpoint hint",
	s3_path_style: "Path-style addressing",
	s3_path_style_desc: "Use /bucket/key requests.",
	secret_key: "Secret key",
	endpoint: "Endpoint",
	policy_editor_credentials_keep_placeholder: "Keep current credential",
};

const t: Translate = (key) => labels[key] ?? key;

vi.mock("@/components/ui/input", () => ({
	Input: ({
		"aria-invalid": ariaInvalid,
		id,
		onBlur,
		onChange,
		placeholder,
		required,
		type,
		value,
	}: {
		"aria-invalid"?: boolean;
		id?: string;
		onBlur?: () => void;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		required?: boolean;
		type?: string;
		value?: string;
	}) => (
		<input
			aria-invalid={ariaInvalid}
			id={id}
			onBlur={onBlur}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			placeholder={placeholder}
			required={required}
			type={type}
			value={value}
		/>
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
	}: {
		children: React.ReactNode;
		htmlFor?: string;
	}) => <label htmlFor={htmlFor}>{children}</label>,
}));

vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		checked,
		id,
		onCheckedChange,
	}: {
		checked: boolean;
		id?: string;
		onCheckedChange?: (checked: boolean) => void;
	}) => (
		<button
			type="button"
			aria-label={`switch:${id ?? "toggle"}:${checked}`}
			onClick={() => onCheckedChange?.(!checked)}
		/>
	),
}));

function renderObjectStorageConnectionFields(
	form: React.ComponentProps<typeof ObjectStorageConnectionFields>["form"],
	onFieldChange = vi.fn(),
	options: Partial<
		Pick<
			React.ComponentProps<typeof ObjectStorageConnectionFields>,
			| "bucketError"
			| "endpointValidationMessage"
			| "isCreateMode"
			| "showCreateValidation"
			| "storageDriverDescriptor"
		>
	> = {},
) {
	render(
		<ObjectStorageConnectionFields
			bucketError={options.bucketError ?? null}
			endpointValidationMessage={options.endpointValidationMessage ?? null}
			form={form}
			isCreateMode={options.isCreateMode ?? true}
			onFieldChange={onFieldChange}
			onSyncNormalizedObjectStorageForm={vi.fn()}
			showCreateValidation={options.showCreateValidation}
			storageDriverDescriptor={
				options.storageDriverDescriptor ??
				objectStorageDescriptor(form.driver_type)
			}
			t={t}
		/>,
	);
	return onFieldChange;
}

function objectStorageDescriptor(
	driverType: DriverType,
): StorageConnectorDescriptor {
	const endpointDisplay =
		driverType === "tencent_cos"
			? {
					help_key: "cos_endpoint_hint",
					placeholder: "https://<bucket-appid>.cos.<region>.myqcloud.com",
				}
			: driverType === "azure_blob"
				? {
						help_key: "azure_blob_endpoint_hint",
						placeholder: "https://<account>.blob.core.windows.net",
					}
				: {
						help_key: "s3_endpoint_hint",
						placeholder: "https://s3.amazonaws.com",
					};
	const fields = [
		{
			invalid_protocol_message_key:
				driverType === "azure_blob"
					? "azure_blob_endpoint_protocol_required_error"
					: "s3_endpoint_protocol_required_error",
			kind: "text",
			label_key: "endpoint",
			name: "endpoint",
			required: true,
			scope: "connection",
			secret: false,
			...endpointDisplay,
			options: [],
			visible_when_driver_types: [],
		},
		{
			kind: "text",
			label_key: "bucket",
			name: "bucket",
			required: true,
			required_message_key:
				driverType === "azure_blob"
					? "policy_wizard_container_required"
					: "policy_wizard_bucket_required",
			scope: "connection",
			secret: false,
			options: [],
			visible_when_driver_types: [],
		},
		{
			kind: "text",
			label_key:
				driverType === "azure_blob" ? "azure_blob_account_name" : "access_key",
			name: "access_key",
			required: true,
			scope: "connection",
			secret: false,
			trim_on_blur: driverType === "azure_blob",
			options: [],
			visible_when_driver_types: [],
		},
		{
			kind: "secret",
			label_key:
				driverType === "azure_blob" ? "azure_blob_account_key" : "secret_key",
			name: "secret_key",
			required: true,
			scope: "connection",
			secret: true,
			options: [],
			visible_when_driver_types: [],
		},
	];
	if (driverType === "s3") {
		fields.push({
			help_key: "s3_path_style_desc",
			kind: "boolean",
			label_key: "s3_path_style",
			name: "s3_path_style",
			required: false,
			scope: "policy_options",
			secret: false,
			options: [],
			visible_when_driver_types: ["s3"],
		});
	}
	return { driver_type: driverType, fields } as StorageConnectorDescriptor;
}

describe("ObjectStorageConnectionFields", () => {
	it("shows the path-style switch for generic S3 policies", () => {
		const onFieldChange = renderObjectStorageConnectionFields({
			...emptyForm,
			driver_type: "s3",
			s3_path_style: true,
		});

		expect(screen.getByText("Path-style addressing")).toBeInTheDocument();
		expect(screen.getByText("Use /bucket/key requests.")).toBeInTheDocument();
		fireEvent.click(screen.getByLabelText("switch:s3_path_style:true"));
		expect(onFieldChange).toHaveBeenCalledWith("s3_path_style", false);
	});

	it("hides the path-style switch for Tencent COS policies", () => {
		renderObjectStorageConnectionFields({
			...emptyForm,
			driver_type: "tencent_cos",
		});

		expect(screen.queryByText("Path-style addressing")).not.toBeInTheDocument();
	});

	it("uses Tencent COS endpoint copy without showing the S3 path-style switch", () => {
		renderObjectStorageConnectionFields({
			...emptyForm,
			driver_type: "tencent_cos",
		});

		expect(screen.getByText("COS endpoint hint")).toBeInTheDocument();
		expect(screen.getByLabelText("Endpoint")).toHaveAttribute(
			"placeholder",
			"https://<bucket-appid>.cos.<region>.myqcloud.com",
		);
		expect(screen.queryByText("Path-style addressing")).not.toBeInTheDocument();
	});

	it("uses Azure Blob account labels and edit placeholders", () => {
		renderObjectStorageConnectionFields(
			{
				...emptyForm,
				driver_type: "azure_blob",
			},
			vi.fn(),
			{
				bucketError: "Container is required",
				endpointValidationMessage: "Endpoint must include protocol",
				isCreateMode: false,
				showCreateValidation: true,
			},
		);

		expect(screen.getByText("Azure Blob endpoint hint")).toBeInTheDocument();
		expect(screen.getByLabelText("Endpoint")).toHaveAttribute(
			"placeholder",
			"https://<account>.blob.core.windows.net",
		);
		expect(screen.getByLabelText("Endpoint")).toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(
			screen.getByText("Endpoint must include protocol"),
		).toBeInTheDocument();
		expect(screen.getByLabelText("Bucket")).toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(screen.getByText("Container is required")).toBeInTheDocument();
		expect(screen.getByLabelText("Account name")).toHaveAttribute(
			"placeholder",
			"Keep current credential",
		);
		expect(screen.getByLabelText("Account key")).toHaveAttribute(
			"placeholder",
			"Keep current credential",
		);
		expect(screen.queryByText("Path-style addressing")).not.toBeInTheDocument();
	});

	it("trims Azure Blob account names on blur", () => {
		const onFieldChange = renderObjectStorageConnectionFields({
			...emptyForm,
			driver_type: "azure_blob",
			access_key: " account-name ",
		});

		fireEvent.blur(screen.getByLabelText("Account name"));

		expect(onFieldChange).toHaveBeenCalledWith("access_key", "account-name");
	});

	it("does not trim S3 access keys on blur", () => {
		const onFieldChange = renderObjectStorageConnectionFields({
			...emptyForm,
			driver_type: "s3",
			access_key: " s3-key ",
		});

		fireEvent.blur(screen.getByLabelText("Access key"));

		expect(onFieldChange).not.toHaveBeenCalledWith("access_key", "s3-key");
	});
});

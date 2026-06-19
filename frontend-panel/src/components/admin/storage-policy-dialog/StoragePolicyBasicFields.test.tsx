import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { StorageConnectorDescriptor } from "@/types/api";
import { emptyForm } from "./formTypes";
import {
	DefaultPolicyToggle,
	LimitsFields,
	LocalContentDedupField,
	PolicyBasePathField,
	PolicyNameField,
	StorageNativeProcessingField,
} from "./StoragePolicyBasicFields";
import type { Translate } from "./StoragePolicyFieldTypes";

const t: Translate = (key) => key;
const localDescriptor = {
	ui: {
		base_path_placeholder: "./data",
	},
} as StorageConnectorDescriptor;

vi.mock("@/components/ui/input", () => ({
	Input: ({
		"aria-invalid": ariaInvalid,
		className,
		id,
		onChange,
		placeholder,
		required,
		type,
		value,
	}: {
		"aria-invalid"?: boolean;
		className?: string;
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		required?: boolean;
		type?: string;
		value?: string | number;
	}) => (
		<input
			aria-invalid={ariaInvalid}
			className={className}
			id={id}
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

describe("StoragePolicyBasicFields", () => {
	it("renders basic policy fields and emits typed field changes", () => {
		const onFieldChange = vi.fn();
		const form = {
			...emptyForm,
			base_path: "/srv/data",
			chunk_size: "8",
			content_dedup: true,
			is_default: false,
			max_file_size: "2048",
			name: "",
		};

		render(
			<>
				<PolicyNameField
					error="name required"
					form={form}
					onFieldChange={onFieldChange}
					showCreateValidation
					t={t}
				/>
				<PolicyBasePathField
					form={form}
					onFieldChange={onFieldChange}
					storageDriverDescriptor={localDescriptor}
					t={t}
				/>
				<LocalContentDedupField
					form={form}
					onFieldChange={onFieldChange}
					t={t}
				/>
				<LimitsFields form={form} onFieldChange={onFieldChange} t={t} />
				<DefaultPolicyToggle form={form} onFieldChange={onFieldChange} t={t} />
			</>,
		);

		expect(screen.getByLabelText("core:name")).toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(screen.getByText("name required")).toBeInTheDocument();
		expect(screen.getByLabelText("base_path")).toHaveAttribute(
			"placeholder",
			"./data",
		);
		expect(screen.getByText("local_content_dedup_desc")).toBeInTheDocument();
		expect(screen.getByLabelText("max_file_size (bytes)")).toHaveAttribute(
			"placeholder",
			"0 = core:unlimited",
		);
		expect(screen.getByText("max_file_size_desc")).toBeInTheDocument();
		expect(screen.getByText("chunk_size_desc")).toBeInTheDocument();
		expect(screen.getByTestId("policy-limits-fields")).toHaveClass(
			"grid",
			"md:grid-cols-2",
		);

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Primary" },
		});
		fireEvent.change(screen.getByLabelText("base_path"), {
			target: { value: "/srv/primary" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "switch:content_dedup:true" }),
		);
		fireEvent.change(screen.getByLabelText("max_file_size (bytes)"), {
			target: { value: "4096" },
		});
		fireEvent.change(screen.getByLabelText("chunk_size"), {
			target: { value: "16" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "switch:is_default:false" }),
		);

		expect(onFieldChange).toHaveBeenCalledWith("name", "Primary");
		expect(onFieldChange).toHaveBeenCalledWith("base_path", "/srv/primary");
		expect(onFieldChange).toHaveBeenCalledWith("content_dedup", false);
		expect(onFieldChange).toHaveBeenCalledWith("max_file_size", "4096");
		expect(onFieldChange).toHaveBeenCalledWith("chunk_size", "16");
		expect(onFieldChange).toHaveBeenCalledWith("is_default", true);
	});

	it("keeps storage-native suffix lists as raw comma splits for later normalization", () => {
		const onFieldChange = vi.fn();
		const form = {
			...emptyForm,
			driver_type: "tencent_cos" as const,
			media_metadata_extensions: ["mp4", "mov"],
			storage_native_media_metadata_enabled: true,
			storage_native_processing_enabled: true,
			thumbnail_extensions: ["jpg", "png"],
			thumbnail_processor: "storage_native" as const,
		};

		render(
			<StorageNativeProcessingField
				form={form}
				onFieldChange={onFieldChange}
				t={t}
			/>,
		);

		expect(
			screen.getByRole("button", {
				name: "switch:storage_native_processing_enabled:true",
			}),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", {
				name: "switch:storage_native_media_metadata_enabled:true",
			}),
		).toBeInTheDocument();
		expect(
			screen.getByLabelText("storage_native_thumbnail_extensions"),
		).toHaveDisplayValue("jpg, png");
		expect(
			screen.getByLabelText("storage_native_media_metadata_extensions"),
		).toHaveDisplayValue("mp4, mov");

		fireEvent.change(
			screen.getByLabelText("storage_native_thumbnail_extensions"),
			{
				target: { value: " webp, .gif " },
			},
		);
		fireEvent.change(
			screen.getByLabelText("storage_native_media_metadata_extensions"),
			{
				target: { value: " mp4, .m4a " },
			},
		);

		// The field keeps the user's raw editing tokens; buildPolicyOptions
		// normalizes, validates, and filters suffixes when creating the payload.
		expect(onFieldChange).toHaveBeenCalledWith("thumbnail_extensions", [
			" webp",
			" .gif ",
		]);
		expect(onFieldChange).toHaveBeenCalledWith("media_metadata_extensions", [
			" mp4",
			" .m4a ",
		]);
	});

	it("hides nested storage-native controls until their switches are enabled", () => {
		const onFieldChange = vi.fn();

		const { rerender } = render(
			<StorageNativeProcessingField
				form={{
					...emptyForm,
					driver_type: "tencent_cos",
					storage_native_processing_enabled: false,
				}}
				onFieldChange={onFieldChange}
				t={t}
			/>,
		);

		expect(
			screen.queryByLabelText("storage_native_thumbnail_extensions"),
		).not.toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", {
				name: "switch:storage_native_processing_enabled:false",
			}),
		);
		expect(onFieldChange).toHaveBeenCalledWith(
			"storage_native_processing_enabled",
			true,
		);

		rerender(
			<StorageNativeProcessingField
				form={{
					...emptyForm,
					driver_type: "tencent_cos",
					storage_native_media_metadata_enabled: false,
					storage_native_processing_enabled: true,
					thumbnail_extensions: ["jpg"],
					thumbnail_processor: "storage_native",
				}}
				onFieldChange={onFieldChange}
				t={t}
			/>,
		);

		expect(
			screen.getByLabelText("storage_native_thumbnail_extensions"),
		).toBeInTheDocument();
		expect(
			screen.queryByLabelText("storage_native_media_metadata_extensions"),
		).not.toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", {
				name: "switch:storage_native_media_metadata_enabled:false",
			}),
		);
		expect(onFieldChange).toHaveBeenCalledWith(
			"storage_native_media_metadata_enabled",
			true,
		);
	});
});

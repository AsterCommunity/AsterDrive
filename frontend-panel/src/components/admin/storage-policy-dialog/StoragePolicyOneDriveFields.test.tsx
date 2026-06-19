import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { StoragePolicyCredentialInfo } from "@/types/api";
import { emptyForm } from "./formTypes";
import { OneDriveCredentialPanel } from "./OneDriveCredentialPanel";
import { OneDriveTargetFields } from "./OneDriveTargetFields";
import {
	getDefaultTenant,
	getTenantMode,
	ONE_DRIVE_AUTO_TENANT_MODE,
	ONE_DRIVE_CUSTOM_TENANT_MODE,
} from "./onedriveFieldUtils";
import type { Translate } from "./StoragePolicyFieldTypes";
import { OneDriveConnectionFields } from "./StoragePolicyOneDriveFields";

const mockState = vi.hoisted(() => ({
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	writeTextToClipboard: vi.fn(),
}));

const labels: Record<string, string> = {
	"core:copied_to_clipboard": "Copied",
	onedrive_account_mode: "Account mode",
	onedrive_account_mode_desc: "Choose the Microsoft account target.",
	onedrive_account_mode_group_drive: "Group drive",
	onedrive_account_mode_personal: "Personal",
	onedrive_account_mode_sharepoint_site: "SharePoint site",
	onedrive_account_mode_work_or_school: "Work or school",
	onedrive_advanced_target: "Advanced target",
	onedrive_authorize_action: "Authorize",
	onedrive_client_id: "Client ID",
	onedrive_client_id_keep_placeholder: "Keep current client ID",
	onedrive_client_id_placeholder: "Application client ID",
	onedrive_client_secret: "Client secret",
	onedrive_client_secret_keep_placeholder: "Keep current client secret",
	onedrive_client_secret_placeholder: "Application client secret",
	onedrive_cloud: "Cloud",
	onedrive_cloud_china: "China",
	onedrive_cloud_desc: "Choose Microsoft Graph cloud.",
	onedrive_cloud_global: "Global",
	onedrive_copy_redirect_uri: "Copy redirect URI",
	onedrive_credential_authorized_at: "Authorized at",
	onedrive_credential_desc_authorized: "Authorized credential is saved.",
	onedrive_credential_desc_missing: "Authorize this policy before use.",
	onedrive_credential_loading: "Loading credential",
	onedrive_credential_reauth_required_desc: "Start authorization again.",
	onedrive_credential_reauth_required_title: "Reauthorization required",
	onedrive_credential_reason_invalid_grant: "Refresh token was revoked.",
	onedrive_credential_refreshed_at: "Refreshed at",
	onedrive_credential_status_authorized: "Authorized",
	onedrive_credential_status_missing: "Missing",
	onedrive_credential_status_reauth_required: "Reauthorization required",
	onedrive_credential_title: "OneDrive credential",
	onedrive_credential_validated_at: "Validated at",
	onedrive_drive_id: "Drive ID",
	onedrive_drive_id_desc: "Leave empty for default drive.",
	onedrive_drive_id_placeholder: "me/drive",
	onedrive_group_id: "Group ID",
	onedrive_group_id_desc: "Microsoft 365 group identifier.",
	onedrive_group_id_placeholder: "group-id",
	onedrive_reauthorize_action: "Reauthorize",
	onedrive_redirect_uri: "Redirect URI",
	onedrive_redirect_uri_desc: "Register this redirect URI in Azure.",
	onedrive_root_item_id: "Root item ID",
	onedrive_root_item_id_desc: "Root folder item identifier.",
	onedrive_root_item_id_placeholder: "root",
	onedrive_setup_notice_cloud: "Choose the matching Graph cloud.",
	onedrive_setup_notice_permissions: "Grant file permissions.",
	onedrive_setup_notice_personal_china:
		"Personal accounts are unavailable in China cloud.",
	onedrive_setup_notice_redirect_uri: "Register the redirect URI.",
	onedrive_setup_notice_title: "Microsoft Graph setup",
	onedrive_site_id: "Site ID",
	onedrive_site_id_desc: "SharePoint site identifier.",
	onedrive_site_id_placeholder: "site-id",
	onedrive_validate_action: "Validate",
};

const t: Translate = (key, values) => {
	const value = labels[key] ?? key;
	if (values?.time) {
		return `${value}: ${values.time}`;
	}
	return value;
};

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: (...args: unknown[]) =>
		mockState.writeTextToClipboard(...args),
}));

vi.mock("@/components/common/AnimatedCollapsible", () => ({
	AnimatedCollapsible: ({
		children,
		open,
	}: {
		children: React.ReactNode;
		open: boolean;
	}) => (open ? <div>{children}</div> : null),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		className,
		variant,
	}: {
		children: React.ReactNode;
		className?: string;
		variant?: string;
	}) => (
		<span className={className} data-variant={variant}>
			{children}
		</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		"aria-label": ariaLabel,
		children,
		className,
		disabled,
		onClick,
		title,
		type,
		variant,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
		type?: "button" | "submit";
		variant?: string;
	}) => (
		<button
			type={type ?? "button"}
			aria-label={ariaLabel}
			className={className}
			data-variant={variant}
			disabled={disabled}
			onClick={onClick}
			title={title}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		"aria-describedby": ariaDescribedBy,
		"aria-invalid": ariaInvalid,
		autoComplete,
		id,
		onChange,
		placeholder,
		readOnly,
		required,
		type,
		value,
	}: {
		"aria-describedby"?: string;
		"aria-invalid"?: boolean;
		autoComplete?: string;
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		readOnly?: boolean;
		required?: boolean;
		type?: string;
		value?: string;
	}) => (
		<input
			aria-describedby={ariaDescribedBy}
			aria-invalid={ariaInvalid}
			autoComplete={autoComplete}
			id={id}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			placeholder={placeholder}
			readOnly={readOnly}
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

vi.mock("@/components/ui/select", () => {
	const { createContext, useContext } =
		require("react") as typeof import("react");

	const SelectContext = createContext<{
		onValueChange?: (value?: string) => void;
	}>({});

	return {
		Select: ({
			children,
			onValueChange,
		}: {
			children: React.ReactNode;
			onValueChange?: (value?: string) => void;
		}) => (
			<SelectContext.Provider value={{ onValueChange }}>
				<div>{children}</div>
			</SelectContext.Provider>
		),
		SelectContent: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectItem: ({
			children,
			value,
		}: {
			children: React.ReactNode;
			value: string;
		}) => {
			const context = useContext(SelectContext);

			return (
				<button
					type="button"
					aria-label={`select-item:${value}`}
					onClick={() => context.onValueChange?.(value)}
				>
					{children}
				</button>
			);
		},
		SelectTrigger: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectValue: () => <span>select-value</span>,
	};
});

function renderTargetFields(
	form = emptyForm,
	onFieldChange = vi.fn(),
	accountModeOptions = [
		{ label: "Work or school", value: "work_or_school" as const },
		{ label: "Personal", value: "personal" as const },
		{ label: "SharePoint site", value: "sharepoint_site" as const },
		{ label: "Group drive", value: "group_drive" as const },
	],
) {
	render(
		<OneDriveTargetFields
			accountModeOptions={accountModeOptions}
			form={form}
			onFieldChange={onFieldChange}
			t={t}
		/>,
	);
	return onFieldChange;
}

function renderConnectionFields(
	form = emptyForm,
	onFieldChange = vi.fn(),
	options: Partial<React.ComponentProps<typeof OneDriveConnectionFields>> = {},
) {
	render(
		<OneDriveConnectionFields
			form={form}
			onFieldChange={onFieldChange}
			t={t}
			{...options}
		/>,
	);
	return onFieldChange;
}

function renderCredentialPanel(
	overrides: Partial<React.ComponentProps<typeof OneDriveCredentialPanel>> = {},
) {
	const props: React.ComponentProps<typeof OneDriveCredentialPanel> = {
		authorizationPending: false,
		credentials: [],
		form: emptyForm,
		loading: false,
		onFieldChange: vi.fn(),
		onStartAuthorization: vi.fn(),
		onValidateCredential: vi.fn(),
		redirectUri: "https://drive.example.com/api/storage/oauth/callback",
		t,
		validationPending: false,
		...overrides,
	};
	render(<OneDriveCredentialPanel {...props} />);
	return props;
}

describe("onedriveFieldUtils", () => {
	it("maps account modes to default tenants and detects tenant edit modes", () => {
		expect(getDefaultTenant("personal")).toBe("consumers");
		expect(getDefaultTenant("work_or_school")).toBe("common");
		expect(getDefaultTenant("sharepoint_site")).toBe("organizations");
		expect(getDefaultTenant("group_drive")).toBe("organizations");

		expect(
			getTenantMode({
				...emptyForm,
				onedrive_account_mode: "personal",
				onedrive_tenant: "consumers",
			}),
		).toBe(ONE_DRIVE_AUTO_TENANT_MODE);
		expect(
			getTenantMode({
				...emptyForm,
				onedrive_tenant: " organizations ",
			}),
		).toBe("organizations");
		expect(
			getTenantMode({
				...emptyForm,
				onedrive_tenant: "contoso.onmicrosoft.com",
			}),
		).toBe(ONE_DRIVE_CUSTOM_TENANT_MODE);
	});
});

describe("OneDriveTargetFields", () => {
	it("updates account mode, target identifiers, and account-specific fields", () => {
		const onFieldChange = renderTargetFields({
			...emptyForm,
			onedrive_account_mode: "sharepoint_site",
			onedrive_drive_id: "drive-1",
			onedrive_root_item_id: "",
			onedrive_site_id: "site-1",
			onedrive_tenant: "organizations",
		});

		fireEvent.click(
			screen.getByRole("button", { name: "select-item:personal" }),
		);
		fireEvent.change(screen.getByLabelText("Drive ID"), {
			target: { value: "drive-2" },
		});
		fireEvent.change(screen.getByLabelText("Root item ID"), {
			target: { value: "root-folder" },
		});
		fireEvent.change(screen.getByLabelText("Site ID"), {
			target: { value: "site-2" },
		});

		expect(screen.getByLabelText("Root item ID")).toHaveDisplayValue("root");
		expect(onFieldChange).toHaveBeenCalledWith(
			"onedrive_account_mode",
			"personal",
		);
		expect(onFieldChange).toHaveBeenCalledWith("onedrive_tenant", "consumers");
		expect(onFieldChange).toHaveBeenCalledWith("onedrive_drive_id", "drive-2");
		expect(onFieldChange).toHaveBeenCalledWith(
			"onedrive_root_item_id",
			"root-folder",
		);
		expect(onFieldChange).toHaveBeenCalledWith("onedrive_site_id", "site-2");
	});

	it("preserves a custom tenant while switching account mode and renders group target", () => {
		const onFieldChange = renderTargetFields({
			...emptyForm,
			onedrive_account_mode: "group_drive",
			onedrive_group_id: "group-1",
			onedrive_tenant: "contoso.onmicrosoft.com",
		});

		fireEvent.click(
			screen.getByRole("button", { name: "select-item:sharepoint_site" }),
		);
		fireEvent.change(screen.getByLabelText("Group ID"), {
			target: { value: "group-2" },
		});

		expect(onFieldChange).toHaveBeenCalledWith(
			"onedrive_account_mode",
			"sharepoint_site",
		);
		expect(onFieldChange).not.toHaveBeenCalledWith(
			"onedrive_tenant",
			expect.any(String),
		);
		expect(onFieldChange).toHaveBeenCalledWith("onedrive_group_id", "group-2");
	});
});

describe("OneDriveConnectionFields", () => {
	it("shows create-mode application fields and normalizes China cloud selections", () => {
		const onFieldChange = renderConnectionFields(
			{
				...emptyForm,
				onedrive_account_mode: "personal",
			},
			vi.fn(),
			{
				clientIdError: "Client ID is required",
				clientSecretError: "Client secret is required",
				mode: "create",
				showCreateValidation: true,
			},
		);

		expect(screen.getByText("Microsoft Graph setup")).toBeInTheDocument();
		expect(screen.getByLabelText("Client ID")).toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(screen.getByText("Client ID is required")).toBeInTheDocument();
		expect(screen.getByLabelText("Client secret")).toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(screen.getByText("Client secret is required")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "select-item:china" }));

		expect(onFieldChange).toHaveBeenCalledWith("onedrive_cloud", "china");
		expect(onFieldChange).toHaveBeenCalledWith(
			"onedrive_account_mode",
			"work_or_school",
		);
		expect(onFieldChange).toHaveBeenCalledWith(
			"onedrive_tenant",
			"organizations",
		);
	});

	it("opens advanced target fields in edit mode and omits personal account mode for China cloud", () => {
		const onFieldChange = renderConnectionFields(
			{
				...emptyForm,
				onedrive_cloud: "china",
				onedrive_account_mode: "work_or_school",
			},
			vi.fn(),
			{ mode: "edit" },
		);

		expect(screen.queryByLabelText("Drive ID")).not.toBeInTheDocument();
		expect(screen.queryByText("Personal")).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /Advanced target/ }));
		fireEvent.click(screen.getByRole("button", { name: "select-item:global" }));

		expect(screen.getByLabelText("Drive ID")).toBeInTheDocument();
		expect(screen.queryByText("Personal")).not.toBeInTheDocument();
		expect(onFieldChange).toHaveBeenCalledWith("onedrive_cloud", "global");
		expect(onFieldChange).toHaveBeenCalledWith("onedrive_tenant", "common");
	});

	it("includes personal account targets for the global cloud in edit mode", () => {
		renderConnectionFields(
			{
				...emptyForm,
				onedrive_cloud: "global",
			},
			vi.fn(),
			{ mode: "edit" },
		);

		fireEvent.click(screen.getByRole("button", { name: /Advanced target/ }));

		expect(screen.getByText("Personal")).toBeInTheDocument();
	});
});

describe("OneDriveCredentialPanel", () => {
	beforeEach(() => {
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.writeTextToClipboard.mockReset();
	});

	it("renders missing credential state, disables validation, and copies redirect URI", async () => {
		mockState.writeTextToClipboard.mockResolvedValue(undefined);
		const onStartAuthorization = vi.fn();
		const onValidateCredential = vi.fn();
		renderCredentialPanel({ onStartAuthorization, onValidateCredential });

		expect(screen.getByText("Missing")).toBeInTheDocument();
		expect(
			screen.getByText("Authorize this policy before use."),
		).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /Authorize/ })).toBeEnabled();
		expect(screen.getByRole("button", { name: /Validate/ })).toBeDisabled();

		fireEvent.click(screen.getByRole("button", { name: /Authorize/ }));
		fireEvent.click(screen.getByRole("button", { name: "Copy redirect URI" }));

		await waitFor(() => {
			expect(mockState.writeTextToClipboard).toHaveBeenCalledWith(
				"https://drive.example.com/api/storage/oauth/callback",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("Copied");
		expect(onStartAuthorization).toHaveBeenCalledTimes(1);
		expect(onValidateCredential).not.toHaveBeenCalled();
	});

	it("renders reauthorization details and saved credential placeholders", () => {
		const onStartAuthorization = vi.fn();
		const onValidateCredential = vi.fn();
		const credential: StoragePolicyCredentialInfo = {
			account_label: "Ada Lovelace",
			authorized_at: "2026-03-28T00:00:00Z",
			expires_at: null,
			last_error: null,
			last_refreshed_at: "2026-03-29T00:00:00Z",
			last_validated_at: "2026-03-30T00:00:00Z",
			provider: "microsoft_graph",
			status: "reauth_required",
			status_reason: "invalid_grant",
			subject: "ada@example.com",
		};

		renderCredentialPanel({
			credentials: [credential],
			onStartAuthorization,
			onValidateCredential,
			validationPending: true,
		});

		expect(screen.getAllByText("Reauthorization required")).toHaveLength(2);
		expect(screen.getByText("Refresh token was revoked.")).toBeInTheDocument();
		expect(screen.getByText("Ada Lovelace")).toBeInTheDocument();
		expect(screen.getByText(/Authorized at:/)).toBeInTheDocument();
		expect(screen.getByLabelText("Client ID")).toHaveAttribute(
			"placeholder",
			"Keep current client ID",
		);
		expect(screen.getByLabelText("Client secret")).toHaveAttribute(
			"placeholder",
			"Keep current client secret",
		);
		expect(screen.getByRole("button", { name: /Reauthorize/ })).toBeEnabled();
		expect(screen.getByRole("button", { name: /Validate/ })).toBeDisabled();
	});

	it("surfaces clipboard failures as toast errors", async () => {
		mockState.writeTextToClipboard.mockRejectedValue(new Error("denied"));
		renderCredentialPanel();

		fireEvent.click(screen.getByRole("button", { name: "Copy redirect URI" }));

		await waitFor(() => {
			expect(mockState.toastError).toHaveBeenCalledWith("denied");
		});
	});
});

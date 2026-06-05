import { fireEvent, render, screen, within } from "@testing-library/react";
import { type ReactNode, use } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	type AdminSettingsCategoryContentProps,
	AdminSettingsCategoryContentProvider,
} from "@/components/admin/settings/AdminSettingsCategoryContentContext";
import {
	CustomConfigRow,
	NewCustomRow,
	SystemConfigRow,
} from "@/components/admin/settings/AdminSettingsConfigRows";
import type {
	ConfigSchemaItem,
	SystemConfig,
	SystemConfigVisibility,
} from "@/types/api";

const mockState = vi.hoisted(() => ({
	updateDraftValue: vi.fn(),
	updateCustomVisibilityDraft: vi.fn(),
	updateNewCustomRow: vi.fn(),
	markCustomDeleted: vi.fn(),
	removeNewCustomRow: vi.fn(),
	navigateToMailSettings: vi.fn(),
	openTemplateVariablesDialog: vi.fn(),
}));

const originalLocation = window.location;

vi.mock("@/components/admin/MediaProcessingConfigEditor", () => ({
	MediaProcessingConfigEditor: () => (
		<div data-testid="media-processing-editor" />
	),
}));

vi.mock("@/components/admin/OfflineDownloadEngineRegistryEditor", () => ({
	OfflineDownloadEngineRegistryEditor: () => (
		<div data-testid="offline-download-editor" />
	),
}));

vi.mock("@/components/admin/PreviewAppsConfigEditor", () => ({
	PreviewAppsConfigEditor: () => <div data-testid="preview-apps-editor" />,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span data-icon={name} />,
}));

vi.mock("@/components/ui/select", () => {
	const React = require("react") as typeof import("react");
	const SelectContext = React.createContext<{
		items: Array<{ label: string; value: string }>;
		onValueChange?: (value: string) => void;
		value?: string;
	} | null>(null);

	return {
		Select: ({
			children,
			items = [],
			onValueChange,
			value,
		}: {
			children: ReactNode;
			items?: Array<{ label: string; value: string }>;
			onValueChange?: (value: string) => void;
			value?: string;
		}) => (
			<SelectContext.Provider value={{ items, onValueChange, value }}>
				{children}
			</SelectContext.Provider>
		),
		SelectContent: ({ children }: { children: ReactNode }) => (
			<div>{children}</div>
		),
		SelectItem: ({
			children,
			value,
		}: {
			children: ReactNode;
			value: string;
		}) => {
			const context = use(SelectContext);
			return (
				<button type="button" onClick={() => context?.onValueChange?.(value)}>
					{children}
				</button>
			);
		},
		SelectTrigger: ({
			children,
			...props
		}: {
			children: ReactNode;
			[key: string]: unknown;
		}) => {
			const context = use(SelectContext);
			return (
				<select
					{...props}
					value={context?.value ?? ""}
					onChange={(event) =>
						context?.onValueChange?.(event.currentTarget.value)
					}
				>
					{context?.items.map((item) => (
						<option key={item.value} value={item.value}>
							{item.label}
						</option>
					))}
					{children}
				</select>
			);
		},
		SelectValue: () => null,
	};
});

vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		checked,
		disabled,
		onCheckedChange,
		...props
	}: {
		checked?: boolean;
		disabled?: boolean;
		onCheckedChange?: (checked: boolean) => void;
		[key: string]: unknown;
	}) => (
		<button
			type="button"
			aria-checked={checked}
			disabled={disabled}
			role="switch"
			onClick={() => onCheckedChange?.(!checked)}
			{...props}
		/>
	),
}));

function createConfig(overrides: Partial<SystemConfig> = {}): SystemConfig {
	return {
		category: "site",
		description: "",
		id: 1,
		is_sensitive: false,
		key: "public_site_url",
		namespace: "site",
		requires_restart: false,
		source: "system",
		updated_at: "2026-04-15T00:00:00Z",
		updated_by: null,
		value: ["https://old.example.com"],
		value_type: "string_array",
		visibility: "private",
		...overrides,
	};
}

function createContextValue(
	overrides: Partial<AdminSettingsCategoryContentProps> = {},
): AdminSettingsCategoryContentProps {
	const draftValues = new Map<string, string | string[]>();
	return {
		activeTab: "site",
		addCustomDraftRow: vi.fn(),
		category: "site",
		configValidationErrors: new Map(),
		deletedCustomConfigs: [],
		displayUnits: {},
		editorTheme: "vs",
		expandedSubcategoryGroups: {},
		expandedTemplateGroups: {},
		getCategoryDescription: () => undefined,
		getCategoryLabel: (category) => category,
		getCustomVisibilityDraft: (config) => config.visibility,
		getDraftValue: (config) =>
			draftValues.get(config.key) ?? (config.value as string | string[]),
		getDraftValueByKey: (key) => draftValues.get(key),
		getMailTemplateGroupLabel: (groupId) => groupId,
		getSubcategoryDescription: () => undefined,
		getSubcategoryLabel: (category, subcategory) => subcategory ?? category,
		getSystemConfigDescription: (config) => config.description || undefined,
		getSystemConfigLabel: (config) => config.key,
		getSystemConfigSchema: () => undefined,
		handleBuildWopiDiscoveryPreviewConfig: vi.fn(),
		handleTestAria2Rpc: vi.fn(),
		handleTestFfmpegCliCommand: vi.fn(),
		handleTestFfprobeCliCommand: vi.fn(),
		handleTestVipsCliCommand: vi.fn(),
		isMobileNavigation: false,
		markCustomDeleted: mockState.markCustomDeleted,
		navigateToMailSettings: mockState.navigateToMailSettings,
		newCustomRowErrors: new Map(),
		newCustomRows: [],
		openTemplateVariablesDialog: mockState.openTemplateVariablesDialog,
		openTestEmailDialog: vi.fn(),
		removeNewCustomRow: mockState.removeNewCustomRow,
		restoreDeletedCustom: vi.fn(),
		setDisplayUnits: vi.fn(),
		systemGroups: {},
		systemSubcategoryGroups: {},
		t: (key, options) =>
			key === "settings_enum_set_selected_count"
				? `${options?.count}/${options?.total} selected`
				: key === "settings_enum_set_visible_count"
					? `${options?.selected}/${options?.count} visible`
					: key,
		tabDirection: "forward",
		toggleSubcategoryGroup: vi.fn(),
		toggleTemplateGroup: vi.fn(),
		updateCustomVisibilityDraft: mockState.updateCustomVisibilityDraft,
		updateDraftValue: mockState.updateDraftValue,
		updateNewCustomRow: mockState.updateNewCustomRow,
		visibleCustomConfigs: [],
		...overrides,
	};
}

function renderWithContext(
	children: ReactNode,
	contextOverrides: Partial<AdminSettingsCategoryContentProps> = {},
) {
	return render(
		<AdminSettingsCategoryContentProvider
			value={createContextValue(contextOverrides)}
		>
			{children}
		</AdminSettingsCategoryContentProvider>,
	);
}

describe("AdminSettingsConfigRows", () => {
	beforeEach(() => {
		mockState.updateDraftValue.mockReset();
		mockState.updateCustomVisibilityDraft.mockReset();
		mockState.updateNewCustomRow.mockReset();
		mockState.markCustomDeleted.mockReset();
		mockState.removeNewCustomRow.mockReset();
		mockState.navigateToMailSettings.mockReset();
		mockState.openTemplateVariablesDialog.mockReset();
	});

	afterEach(() => {
		Object.defineProperty(window, "location", {
			configurable: true,
			value: originalLocation,
		});
	});

	it("adds the current origin to an empty public site URL row", () => {
		Object.defineProperty(window, "location", {
			configurable: true,
			value: new URL("https://drive.example.com/app"),
		});
		const config = createConfig({ value: [] });

		renderWithContext(<SystemConfigRow config={config} />);

		fireEvent.click(
			screen.getByRole("button", {
				name: "public_site_url_add_current_origin",
			}),
		);

		expect(mockState.updateDraftValue).toHaveBeenCalledWith("public_site_url", [
			"https://drive.example.com",
		]);
	});

	it("edits and clears string array rows without keeping a blank-only draft", () => {
		const config = createConfig({
			key: "auth_local_email_allowlist",
			value: ["ops@example.com"],
		});

		renderWithContext(<SystemConfigRow config={config} />);

		fireEvent.change(
			screen.getByRole("textbox", {
				name: "local_email_policy_item_label 1",
			}),
			{ target: { value: "admin@example.com" } },
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "local_email_policy_remove_item",
			}),
		);

		expect(mockState.updateDraftValue).toHaveBeenNthCalledWith(
			1,
			"auth_local_email_allowlist",
			["admin@example.com"],
		);
		expect(mockState.updateDraftValue).toHaveBeenNthCalledWith(
			2,
			"auth_local_email_allowlist",
			[],
		);
	});

	it("filters enum-set options and preserves schema order when toggling", () => {
		const config = createConfig({
			key: "preview_enabled_mime_types",
			value: ["image/png"],
			value_type: "string_enum_set",
		});
		const schema: ConfigSchemaItem = {
			category: "site.preview",
			description: "",
			description_i18n_key: "preview.desc",
			is_sensitive: false,
			key: config.key,
			label_i18n_key: "preview.label",
			options: [
				{
					group: "images",
					label_i18n_key: "settings_option_image_png",
					value: "image/png",
				},
				{
					group: "documents",
					label_i18n_key: "settings_option_application_pdf",
					value: "application/pdf",
				},
			],
			requires_restart: false,
			value_type: "string_enum_set",
		};

		renderWithContext(<SystemConfigRow config={config} />, {
			getSystemConfigSchema: () => schema,
		});

		fireEvent.change(
			screen.getByPlaceholderText("settings_enum_set_search_placeholder"),
			{ target: { value: "pdf" } },
		);
		expect(screen.getByText("0/1 visible")).toBeInTheDocument();
		expect(screen.queryByText("image/png")).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /application\/pdf/ }));

		expect(mockState.updateDraftValue).toHaveBeenCalledWith(
			"preview_enabled_mime_types",
			["image/png", "application/pdf"],
		);
	});

	it("selects visible enum-set options only from the filtered result", () => {
		const config = createConfig({
			key: "preview_enabled_mime_types",
			value: [],
			value_type: "string_enum_set",
		});
		const schema: ConfigSchemaItem = {
			category: "site.preview",
			description: "",
			description_i18n_key: "preview.desc",
			is_sensitive: false,
			key: config.key,
			label_i18n_key: "preview.label",
			options: [
				{
					group: "images",
					label_i18n_key: "PNG image",
					value: "image/png",
				},
				{
					group: "documents",
					label_i18n_key: "PDF document",
					value: "application/pdf",
				},
			],
			requires_restart: false,
			value_type: "string_enum_set",
		};

		renderWithContext(<SystemConfigRow config={config} />, {
			getSystemConfigSchema: () => schema,
			t: (key, options) =>
				key === "PNG image" || key === "PDF document"
					? key
					: key === "settings_enum_set_selected_count"
						? `${options?.count}/${options?.total} selected`
						: key === "settings_enum_set_visible_count"
							? `${options?.selected}/${options?.count} visible`
							: key,
		});

		fireEvent.change(
			screen.getByPlaceholderText("settings_enum_set_search_placeholder"),
			{ target: { value: "document" } },
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings_enum_set_select_visible",
			}),
		);

		expect(mockState.updateDraftValue).toHaveBeenCalledWith(
			"preview_enabled_mime_types",
			["application/pdf"],
		);
	});

	it("blocks email code login until mail delivery config is ready", () => {
		const config = createConfig({
			key: "auth_email_code_login_enabled",
			value: "false",
			value_type: "boolean",
		});

		renderWithContext(<SystemConfigRow config={config} />, {
			getDraftValueByKey: () => undefined,
		});

		expect(screen.getByRole("switch")).toBeDisabled();
		expect(
			screen.getByText("email_code_mfa_mail_config_required"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: "email_code_mfa_mail_settings_link",
			}),
		);

		expect(mockState.navigateToMailSettings).toHaveBeenCalledTimes(1);
		expect(mockState.updateDraftValue).not.toHaveBeenCalled();
	});

	it("updates custom row visibility and deletion actions", () => {
		const config = createConfig({
			key: "custom.theme",
			source: "custom",
			value: "ocean",
			visibility: "private",
		});

		renderWithContext(<CustomConfigRow config={config} />, {
			getCustomVisibilityDraft: () => "authenticated",
		});

		fireEvent.change(screen.getByLabelText("custom_config_visibility"), {
			target: { value: "public" },
		});
		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));

		expect(screen.getByText("settings_status_unsaved")).toBeInTheDocument();
		expect(mockState.updateCustomVisibilityDraft).toHaveBeenCalledWith(
			"custom.theme",
			"public",
		);
		expect(mockState.markCustomDeleted).toHaveBeenCalledWith("custom.theme");
	});

	it("updates and removes a new custom row", () => {
		renderWithContext(
			<NewCustomRow
				row={{
					id: "row-1",
					key: "",
					value: "",
					visibility: "private",
				}}
			/>,
			{
				newCustomRowErrors: new Map([["row-1", "custom_config_key_required"]]),
			},
		);

		fireEvent.change(
			screen.getByPlaceholderText("custom_config_key_placeholder"),
			{
				target: { value: "my.theme" },
			},
		);
		fireEvent.change(screen.getByPlaceholderText("config_value"), {
			target: { value: "dark" },
		});
		fireEvent.change(screen.getByLabelText("custom_config_visibility"), {
			target: { value: "authenticated" satisfies SystemConfigVisibility },
		});
		const deleteButtons = screen.getAllByRole("button", {
			name: "core:delete",
		});
		fireEvent.click(deleteButtons[0]);

		expect(screen.getByText("custom_config_key_required")).toBeInTheDocument();
		expect(mockState.updateNewCustomRow).toHaveBeenCalledWith(
			"row-1",
			"key",
			"my.theme",
		);
		expect(mockState.updateNewCustomRow).toHaveBeenCalledWith(
			"row-1",
			"value",
			"dark",
		);
		expect(mockState.updateNewCustomRow).toHaveBeenCalledWith(
			"row-1",
			"visibility",
			"authenticated",
		);
		expect(mockState.removeNewCustomRow).toHaveBeenCalledWith("row-1");
	});

	it("marks a template config as unsaved and opens template variables", () => {
		const config = createConfig({
			category: "mail.template",
			key: "mail_password_reset_html",
			value: "<p>Old</p>",
			value_type: "multiline",
		});

		renderWithContext(<SystemConfigRow config={config} />, {
			getDraftValue: () => "<p>New</p>",
		});

		fireEvent.click(
			screen.getByRole("button", { name: "mail_template_variable_link" }),
		);

		expect(screen.getByText("settings_status_unsaved")).toBeInTheDocument();
		expect(mockState.openTemplateVariablesDialog).toHaveBeenCalledWith(config);
	});

	it("renders no-options and no-matches states for enum-set configs", () => {
		const config = createConfig({
			key: "empty_enum_set",
			value: [],
			value_type: "string_enum_set",
		});
		const schema: ConfigSchemaItem = {
			category: "site.preview",
			description: "",
			description_i18n_key: "preview.desc",
			is_sensitive: false,
			key: config.key,
			label_i18n_key: "preview.label",
			options: [],
			requires_restart: false,
			value_type: "string_enum_set",
		};

		const { rerender } = renderWithContext(
			<SystemConfigRow config={config} />,
			{
				getSystemConfigSchema: () => schema,
			},
		);

		expect(
			screen.getByText("settings_enum_set_no_options"),
		).toBeInTheDocument();

		const populatedSchema = {
			...schema,
			options: [
				{
					group: "images",
					label_i18n_key: "PNG image",
					value: "image/png",
				},
			],
		};
		rerender(
			<AdminSettingsCategoryContentProvider
				value={createContextValue({
					getSystemConfigSchema: () => populatedSchema,
				})}
			>
				<SystemConfigRow config={config} />
			</AdminSettingsCategoryContentProvider>,
		);

		fireEvent.change(
			screen.getByPlaceholderText("settings_enum_set_search_placeholder"),
			{ target: { value: "pdf" } },
		);

		expect(
			screen.getByText("settings_enum_set_no_matches"),
		).toBeInTheDocument();
	});

	it("selects all and clears enum-set options", () => {
		const config = createConfig({
			key: "preview_enabled_mime_types",
			value: ["image/png"],
			value_type: "string_enum_set",
		});
		const schema: ConfigSchemaItem = {
			category: "site.preview",
			description: "",
			description_i18n_key: "preview.desc",
			is_sensitive: false,
			key: config.key,
			label_i18n_key: "preview.label",
			options: [
				{
					group: "images",
					label_i18n_key: "PNG image",
					value: "image/png",
				},
				{
					group: "documents",
					label_i18n_key: "PDF document",
					value: "application/pdf",
				},
			],
			requires_restart: false,
			value_type: "string_enum_set",
		};

		renderWithContext(<SystemConfigRow config={config} />, {
			getSystemConfigSchema: () => schema,
		});

		fireEvent.click(
			screen.getByRole("button", { name: "settings_enum_set_select_all" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "settings_enum_set_clear" }),
		);

		expect(mockState.updateDraftValue).toHaveBeenNthCalledWith(
			1,
			"preview_enabled_mime_types",
			["image/png", "application/pdf"],
		);
		expect(mockState.updateDraftValue).toHaveBeenNthCalledWith(
			2,
			"preview_enabled_mime_types",
			[],
		);
	});

	it("shows selected counts per enum-set group", () => {
		const config = createConfig({
			key: "preview_enabled_mime_types",
			value: ["image/png"],
			value_type: "string_enum_set",
		});
		const schema: ConfigSchemaItem = {
			category: "site.preview",
			description: "",
			description_i18n_key: "preview.desc",
			is_sensitive: false,
			key: config.key,
			label_i18n_key: "preview.label",
			options: [
				{
					group: "images",
					label_i18n_key: "PNG image",
					value: "image/png",
				},
			],
			requires_restart: false,
			value_type: "string_enum_set",
		};

		renderWithContext(<SystemConfigRow config={config} />, {
			getSystemConfigSchema: () => schema,
		});

		const group = screen.getByText("Images").closest("section");

		expect(group).not.toBeNull();
		expect(within(group as HTMLElement).getByText("1/1")).toBeInTheDocument();
	});
});

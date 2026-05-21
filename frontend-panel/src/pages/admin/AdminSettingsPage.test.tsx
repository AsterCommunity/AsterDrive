import {
	act,
	createEvent,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invalidateAdminConfigMetadataCache } from "@/lib/adminConfigMetadataCache";
import AdminSettingsPage from "@/pages/admin/AdminSettingsPage";
import type {
	ConfigSchemaItem,
	SystemConfig,
	SystemConfigSource,
	SystemConfigValueType,
	TemplateVariableGroup,
} from "@/types/api";

const mockState = vi.hoisted(() => ({
	actionConfig: vi.fn(),
	codeEditorProps: null as null | Record<string, unknown>,
	deleteConfig: vi.fn(),
	handleApiError: vi.fn(),
	listConfigs: vi.fn(),
	navigate: vi.fn(),
	schema: vi.fn(),
	sendTestEmail: vi.fn(),
	setConfig: vi.fn(),
	templateVariables: vi.fn(),
	toastSuccess: vi.fn(),
}));

const defaultLocation = window.location;

const translationMap: Record<string, string> = {
	cors_wildcard_credentials_validation_error:
		"cors_wildcard_credentials_validation_error",
	settings_item_auth_access_token_ttl_secs_desc:
		"Controls how long newly issued access tokens stay valid.",
	settings_item_auth_access_token_ttl_secs_label: "Access token lifetime",
	public_site_url_add_current_origin: "public_site_url_add_current_origin",
	settings_time_unit_label: "Time unit",
	settings_time_unit_seconds: "Seconds",
	settings_time_unit_minutes: "Minutes",
	settings_time_unit_hours: "Hours",
	settings_time_unit_days: "Days",
	settings_time_unit_weeks: "Weeks",
	settings_size_unit_label: "Size unit",
	settings_size_unit_bytes: "B",
	settings_size_unit_kilobytes: "KB",
	settings_size_unit_megabytes: "MB",
	settings_size_unit_gigabytes: "GB",
	settings_size_unit_terabytes: "TB",
	mail_send_test_email: "mail_send_test_email",
	mail_send_test_email_hint: "mail_send_test_email_hint",
	mail_test_email_dialog_desc: "mail_test_email_dialog_desc",
	mail_test_email_dialog_title: "mail_test_email_dialog_title",
	mail_test_email_recipient_label: "mail_test_email_recipient_label",
	mail_test_email_recipient_placeholder:
		"mail_test_email_recipient_placeholder",
	mail_template_variable_link: "mail_template_variable_link",
	mail_template_variables_dialog_desc: "mail_template_variables_dialog_desc",
	mail_template_variables_dialog_empty: "mail_template_variables_dialog_empty",
	mail_template_variables_dialog_title: "mail_template_variables_dialog_title",
	settings_mail_template_group_external_auth_email_verification:
		"External sign-in email verification",
	settings_item_mail_template_external_auth_email_verification_subject_label:
		"External sign-in email verification subject",
	settings_item_mail_template_external_auth_email_verification_subject_desc:
		"Subject template for external sign-in email verification emails.",
	settings_item_mail_template_external_auth_email_verification_html_label:
		"External sign-in email verification HTML body",
	settings_item_mail_template_external_auth_email_verification_html_desc:
		"HTML template for external sign-in email verification emails. A complete HTML document is recommended for best client compatibility.",
	settings_section_collapse: "settings_section_collapse",
	settings_section_expand: "settings_section_expand",
	settings_subcategory_mail_config: "settings_subcategory_mail_config",
	settings_subcategory_mail_template: "settings_subcategory_mail_template",
	settings_subcategory_storage_media_processing: "Media Processing",
	settings_subcategory_storage_media_processing_desc:
		"Configure media metadata, available thumbnail processors, and safety limits.",
	settings_item_media_metadata_enabled_label: "Enable media metadata",
	settings_item_media_metadata_enabled_desc:
		"Allow the backend to parse and cache basic image, audio, and video metadata by blob.",
	settings_item_media_metadata_max_source_bytes_label:
		"Media metadata source-size cap",
	settings_item_media_processing_registry_json_label:
		"Media processing registry",
	settings_item_media_processing_registry_json_desc:
		"Choose which media processors are enabled and bind extensions as needed. Built-in AsterDrive processors act as the final fallback when enabled.",
	media_processing_editor_title: "media_processing_editor_title",
	media_processing_editor_desc: "media_processing_editor_desc",
	media_processing_editor_validation_title:
		"media_processing_editor_validation_title",
	media_processing_editor_processor_disabled:
		"media_processing_editor_processor_disabled",
	media_processing_editor_processor_enabled:
		"media_processing_editor_processor_enabled",
	media_processing_editor_processor_command_desc:
		"media_processing_editor_processor_command_desc",
	media_processing_editor_processor_command_label:
		"media_processing_editor_processor_command_label",
	media_processing_editor_processor_command_placeholder:
		"media_processing_editor_processor_command_placeholder",
	media_processing_editor_processor_test_command:
		"media_processing_editor_processor_test_command",
	media_processing_editor_processor_testing_command:
		"media_processing_editor_processor_testing_command",
	media_processing_editor_processor_enabled_desc:
		"media_processing_editor_processor_enabled_desc",
	thumbnail_processor_images: "AsterDrive Built-in",
	thumbnail_processor_ffmpeg_cli: "ffmpeg_cli",
	thumbnail_processor_ffprobe_cli: "ffprobe_cli",
	thumbnail_processor_lofty: "lofty",
	thumbnail_processor_vips_cli: "vips_cli",
	media_processing_editor_use_thumbnail_image: "Image Thumbnail",
	media_processing_editor_use_thumbnail_audio: "Audio Artwork Thumbnail",
	media_processing_editor_use_thumbnail_video: "Video Thumbnail",
	media_processing_editor_use_metadata_image: "Image Metadata",
	media_processing_editor_use_metadata_audio: "Audio Metadata",
	media_processing_editor_use_metadata_video: "Video Metadata",
	settings_subcategory_storage_archive_extract: "Archive Extraction",
	settings_subcategory_storage_archive_extract_desc:
		"Archive extraction limits.",
	settings_subcategory_storage_archive_preview: "Archive Preview",
	settings_subcategory_storage_archive_preview_desc: "Archive preview limits.",
	settings_save_hint:
		"更改会先暂存为草稿，确认无误后再统一保存，⌘/Ctrl + S 保存。",
	settings_template_variable_reset_url_desc:
		"settings_template_variable_reset_url_desc",
	settings_template_variable_reset_url_label:
		"settings_template_variable_reset_url_label",
	settings_template_variable_email_desc: "The email address being verified.",
	settings_template_variable_email_label: "Email",
	settings_template_variable_expires_in_desc:
		"How long the verification link remains valid after the email is generated.",
	settings_template_variable_expires_in_label: "Expires In",
	settings_template_variable_provider_name_desc:
		"The display name of the external sign-in provider.",
	settings_template_variable_provider_name_label: "Provider Name",
	settings_template_variable_site_name_desc:
		"The current public site name from branding settings.",
	settings_template_variable_site_name_label: "Site Name",
	settings_template_variable_verification_url_desc:
		"The confirmation link for the current verification email.",
	settings_template_variable_verification_url_label: "Verification URL",
	settings_template_variable_username_desc:
		"settings_template_variable_username_desc",
	settings_template_variable_username_label:
		"settings_template_variable_username_label",
};

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "settings_save_notice")
				return `settings_save_notice:${options?.count}`;
			if (key === "mail_test_email_sent")
				return `mail_test_email_sent:${options?.email}`;
			return translationMap[key] ?? key;
		},
	}),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: { user: { email: string } | null }) => unknown,
	) => selector({ user: { email: "admin@example.com" } }),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({ title }: { title: string }) => <div>{title}</div>,
}));

vi.mock("@/components/common/SkeletonTable", () => ({
	SkeletonTable: ({ columns, rows }: { columns: number; rows: number }) => (
		<div>{`skeleton:${columns}:${rows}`}</div>
	),
}));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		title,
		description,
	}: {
		title: string;
		description: string;
	}) => (
		<div>
			<h1>{title}</h1>
			<p>{description}</p>
		</div>
	),
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		"aria-label": ariaLabel,
		children,
		className,
		disabled,
		onClick,
		type,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button
			type={type ?? "button"}
			aria-label={ariaLabel}
			className={className}
			disabled={disabled}
			onClick={onClick}
		>
			{children}
		</button>
	),
	buttonVariants: ({
		className,
	}: {
		className?: string;
		variant?: string;
		size?: string;
	}) => className ?? "",
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		onChange,
		value,
		...props
	}: {
		onChange?: (event: { target: { value: string } }) => void;
		value?: string;
		[key: string]: unknown;
	}) => (
		<input
			{...props}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			value={value}
		/>
	),
}));

vi.mock("@/components/ui/select", async () => {
	const React = await vi.importActual<typeof import("react")>("react");
	const SelectContext = React.createContext<{
		items?: Array<{ label: string; value: string }>;
		onValueChange?: (value: string) => void;
		value?: string;
	}>({});

	return {
		Select: ({
			children,
			items,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			items?: Array<{ label: string; value: string }>;
			onValueChange?: (value: string) => void;
			value?: string;
		}) => (
			<SelectContext.Provider value={{ items, onValueChange, value }}>
				<div>{children}</div>
			</SelectContext.Provider>
		),
		SelectTrigger: ({
			"aria-label": ariaLabel,
			className,
			id,
		}: {
			"aria-label"?: string;
			className?: string;
			id?: string;
			[key: string]: unknown;
		}) => {
			const context = React.useContext(SelectContext);

			return (
				<select
					aria-label={ariaLabel ?? id}
					className={className}
					value={context.value}
					onChange={(event) => context.onValueChange?.(event.target.value)}
				>
					{(context.items ?? []).map((item) => (
						<option key={item.value} value={item.value}>
							{item.label}
						</option>
					))}
				</select>
			);
		},
		SelectValue: () => null,
		SelectContent: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectItem: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
	};
});

vi.mock("@/components/files/preview/CodePreviewEditor", () => ({
	CodePreviewEditor: ({
		options,
		onChange,
		value,
	}: {
		options?: Record<string, unknown>;
		onChange?: (value: string) => void;
		value: string;
	}) => {
		mockState.codeEditorProps = { options, value };

		return (
			<textarea
				aria-label="Code editor"
				value={value}
				onChange={(event) => onChange?.(event.target.value)}
			/>
		);
	},
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({
		children,
		open,
	}: {
		children: React.ReactNode;
		open?: boolean;
	}) => (open ? <div>{children}</div> : null),
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div role="dialog">{children}</div>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogFooter: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
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
			id={id}
			aria-label={`switch:${id ?? "config"}:${checked}`}
			onClick={() => onCheckedChange?.(!checked)}
		/>
	),
}));

vi.mock("@/components/ui/tabs", async () => {
	const React = await vi.importActual<typeof import("react")>("react");
	const TabsContext = React.createContext<{
		onValueChange?: (value: string) => void;
		value?: string;
	}>({});

	return {
		Tabs: ({
			children,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			onValueChange?: (value: string) => void;
			value?: string;
		}) => (
			<TabsContext.Provider value={{ onValueChange, value }}>
				<div>{children}</div>
			</TabsContext.Provider>
		),
		TabsList: ({
			children,
			className,
		}: {
			children: React.ReactNode;
			className?: string;
		}) => <div className={className}>{children}</div>,
		TabsTrigger: ({
			children,
			className,
			value,
		}: {
			children: React.ReactNode;
			className?: string;
			value: string;
		}) => {
			const tabs = React.useContext(TabsContext);

			return (
				<button
					type="button"
					className={className}
					data-active={tabs.value === value ? "" : undefined}
					data-value={value}
					onClick={() => tabs.onValueChange?.(value)}
				>
					{children}
				</button>
			);
		},
		TabsContent: ({
			children,
			value,
		}: {
			children: React.ReactNode;
			value?: string;
		}) => {
			const tabs = React.useContext(TabsContext);
			return tabs.value === value ? <div>{children}</div> : null;
		},
	};
});

vi.mock("@/components/ui/dropdown-menu", () => ({
	DropdownMenu: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DropdownMenuTrigger: ({
		children,
		render,
	}: {
		children?: React.ReactNode;
		render?: React.ReactNode;
	}) => <>{render ?? children}</>,
	DropdownMenuContent: () => null,
	DropdownMenuRadioGroup: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DropdownMenuRadioItem: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/adminService", () => ({
	adminConfigService: {
		action: (...args: unknown[]) => mockState.actionConfig(...args),
		delete: (...args: unknown[]) => mockState.deleteConfig(...args),
		list: (...args: unknown[]) => mockState.listConfigs(...args),
		sendTestEmail: (...args: unknown[]) => mockState.sendTestEmail(...args),
		schema: (...args: unknown[]) => mockState.schema(...args),
		set: (...args: unknown[]) => mockState.setConfig(...args),
		templateVariables: (...args: unknown[]) =>
			mockState.templateVariables(...args),
	},
}));

vi.mock("@/stores/themeStore", () => ({
	useThemeStore: (
		selector: (state: { resolvedTheme: "light" | "dark" }) => unknown,
	) => selector({ resolvedTheme: "light" }),
}));

function createConfig(overrides: Partial<SystemConfig> = {}): SystemConfig {
	return {
		category: "storage",
		description: "desc",
		id: 1,
		is_sensitive: false,
		key: "storage.enabled",
		requires_restart: false,
		source: "system",
		updated_at: "2026-04-15T00:00:00Z",
		updated_by: null,
		value: "true",
		value_type: "boolean",
		...overrides,
	};
}

function createSchemaItem(
	overrides: Partial<ConfigSchemaItem> = {},
): ConfigSchemaItem {
	return {
		category: "storage",
		description: "desc",
		description_i18n_key: "",
		is_sensitive: false,
		key: "storage.enabled",
		label_i18n_key: "",
		requires_restart: false,
		value_type: "boolean",
		...overrides,
	};
}

function createTemplateVariableGroup(
	overrides: Partial<TemplateVariableGroup> = {},
): TemplateVariableGroup {
	return {
		category: "mail.template",
		label_i18n_key: "settings_mail_template_group_password_reset",
		template_code: "password_reset",
		variables: [
			{
				description_i18n_key: "settings_template_variable_username_desc",
				label_i18n_key: "settings_template_variable_username_label",
				token: "{{username}}",
			},
			{
				description_i18n_key: "settings_template_variable_reset_url_desc",
				label_i18n_key: "settings_template_variable_reset_url_label",
				token: "{{reset_url}}",
			},
			{
				description_i18n_key: "settings_template_variable_site_name_desc",
				label_i18n_key: "settings_template_variable_site_name_label",
				token: "{{site_name}}",
			},
		],
		...overrides,
	};
}

function getMockConfigCategory(key: string) {
	if (key.startsWith("auth")) return "auth";
	if (key.startsWith("custom")) return "custom";
	if (key.startsWith("mail_template_")) return "mail.template";
	if (key.startsWith("mail_")) return "mail.config";
	if (key === "media_processing_registry_json")
		return "storage.media_processing";
	if (key.startsWith("media_metadata_")) return "storage.media_processing";
	if (key.startsWith("thumbnail_")) return "storage.media_processing";
	return "storage";
}

function getMockConfigSource(key: string): SystemConfigSource {
	return key.startsWith("custom") ? "custom" : "system";
}

function getMockConfigValueType(key: string): SystemConfigValueType {
	if (key === "storage.enabled") {
		return "boolean";
	}
	if (
		key === "auth_access_token_ttl_secs" ||
		key === "default_storage_quota" ||
		key.endsWith("_bytes")
	) {
		return "number";
	}
	if (key === "media_metadata_enabled") return "boolean";
	if (key === "media_processing_registry_json") return "multiline";
	return "string";
}

describe("AdminSettingsPage", () => {
	beforeEach(() => {
		Object.defineProperty(window, "location", {
			configurable: true,
			value: defaultLocation,
		});
		invalidateAdminConfigMetadataCache();
		mockState.actionConfig.mockReset();
		mockState.codeEditorProps = null;
		mockState.deleteConfig.mockReset();
		mockState.handleApiError.mockReset();
		mockState.listConfigs.mockReset();
		mockState.navigate.mockReset();
		mockState.schema.mockReset();
		mockState.sendTestEmail.mockReset();
		mockState.setConfig.mockReset();
		mockState.templateVariables.mockReset();
		mockState.toastSuccess.mockReset();

		Object.defineProperty(window, "innerWidth", {
			configurable: true,
			value: 1024,
			writable: true,
		});

		mockState.listConfigs.mockResolvedValue({
			items: [
				createConfig(),
				createConfig({
					category: "auth",
					description: "ttl desc",
					key: "auth_access_token_ttl_secs",
					value: "1200",
					value_type: "number",
				}),
				createConfig({
					category: "custom",
					description: "",
					key: "custom.theme",
					source: "custom",
					value: "ocean",
					value_type: "string",
				}),
			],
		});
		mockState.schema.mockResolvedValue([
			createSchemaItem(),
			createSchemaItem({
				category: "auth",
				description: "ttl desc",
				description_i18n_key: "settings_item_auth_access_token_ttl_secs_desc",
				key: "auth_access_token_ttl_secs",
				label_i18n_key: "settings_item_auth_access_token_ttl_secs_label",
				value_type: "number",
			}),
		]);
		mockState.sendTestEmail.mockResolvedValue({
			message: "Test email sent to admin@example.com",
		});
		mockState.actionConfig.mockResolvedValue({
			message: "Imported WOPI discovery apps from 1 source",
		});
		mockState.templateVariables.mockResolvedValue([
			createTemplateVariableGroup(),
		]);
		mockState.setConfig.mockImplementation((key: string, value: string) =>
			Promise.resolve(
				createConfig({
					category: getMockConfigCategory(key),
					key,
					source: getMockConfigSource(key),
					value,
					value_type: getMockConfigValueType(key),
				}),
			),
		);
		mockState.deleteConfig.mockResolvedValue(undefined);
	});

	it("shows a loading state and then an empty state when there are no configs", async () => {
		mockState.listConfigs.mockResolvedValueOnce({ items: [] });
		mockState.schema.mockResolvedValueOnce([]);

		render(<AdminSettingsPage />);

		expect(screen.getByText("skeleton:4:8")).toBeInTheDocument();
		expect(await screen.findByText("no_config")).toBeInTheDocument();
	});

	it("hides duplicate category headings and descriptions on mobile", async () => {
		Object.defineProperty(window, "innerWidth", {
			configurable: true,
			value: 375,
			writable: true,
		});

		render(<AdminSettingsPage />);

		await screen.findByDisplayValue("20");

		expect(
			screen.queryByRole("heading", { name: "settings_category_auth" }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText("settings_category_auth_desc"),
		).not.toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /settings_category_auth/i }),
		).toBeInTheDocument();
	});

	it("keeps category descriptions in the desktop sidebar without repeating them in content", async () => {
		Object.defineProperty(window, "innerWidth", {
			configurable: true,
			value: 1440,
			writable: true,
		});

		render(<AdminSettingsPage section="auth" />);

		await screen.findByDisplayValue("20");

		expect(screen.getAllByText("settings_category_auth_desc")).toHaveLength(1);
		expect(screen.getAllByText("Shield")).toHaveLength(1);
	});

	it("renders category tabs and only saves boolean changes from the bottom action", async () => {
		render(<AdminSettingsPage section="storage" />);

		await screen.findByRole("button", { name: /settings_category_auth/i });
		expect(
			screen.getByRole("button", { name: /settings_category_custom/i }),
		).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "save_changes" }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "core:edit" }),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByLabelText("switch:storage.enabled:true"));

		expect(mockState.setConfig).not.toHaveBeenCalled();
		expect(
			await screen.findByText("settings_save_notice:1"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"storage.enabled",
				"false",
			);
		});
		expect(
			screen.getByLabelText("switch:storage.enabled:false"),
		).toBeInTheDocument();
		expect(mockState.listConfigs).toHaveBeenCalledTimes(1);
		expect(mockState.schema).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("settings_saved");
	});

	it("opens the mail test dialog with the current user email and sends to the edited recipient", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "mail.config",
					key: "mail_smtp_host",
					value: "smtp.example.com",
					value_type: "string",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "mail.config",
				key: "mail_smtp_host",
				value_type: "string",
			}),
		]);
		mockState.sendTestEmail.mockResolvedValueOnce({
			message: "Test email sent to deliver@example.com",
		});

		render(<AdminSettingsPage section="mail" />);

		fireEvent.click(
			await screen.findByRole("button", { name: /mail_send_test_email/i }),
		);

		const dialog = screen.getByRole("dialog");
		expect(
			within(dialog).getByDisplayValue("admin@example.com"),
		).toBeInTheDocument();

		fireEvent.change(within(dialog).getByDisplayValue("admin@example.com"), {
			target: { value: "deliver@example.com" },
		});
		fireEvent.click(
			within(dialog).getAllByRole("button", {
				name: /mail_send_test_email/i,
			})[0],
		);

		await waitFor(() => {
			expect(mockState.sendTestEmail).toHaveBeenCalledWith(
				"deliver@example.com",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"mail_test_email_sent:deliver@example.com",
		);
	});

	it("uses the shared underline accent for compact category tabs", async () => {
		render(<AdminSettingsPage />);

		const storageTab = await screen.findByRole("button", {
			name: /settings_category_storage/i,
		});

		expect(storageTab).toHaveClass("h-10", "rounded-none", "px-0");
		expect(storageTab).not.toHaveClass("after:hidden");

		const tabContent = storageTab.firstElementChild;
		expect(tabContent).not.toBeNull();
		expect(tabContent).not.toHaveClass("group-data-[active]:bg-muted/70");

		const compactTabList = storageTab.parentElement;
		expect(compactTabList).not.toBeNull();
		expect(compactTabList).toHaveClass(
			"overflow-hidden",
			"border-b",
			"border-border/40",
			"pb-2",
		);
		expect(compactTabList).not.toHaveClass("overflow-x-auto");

		const compactNav = compactTabList?.parentElement;
		expect(compactNav).not.toBeNull();
		expect(compactNav).toHaveClass("items-end");
	});

	it("adds a vertical divider for the desktop category sidebar", async () => {
		Object.defineProperty(window, "innerWidth", {
			configurable: true,
			value: 1440,
			writable: true,
		});

		render(<AdminSettingsPage />);

		const storageTab = await screen.findByRole("button", {
			name: /settings_category_storage/i,
		});

		const desktopNav = storageTab.parentElement?.parentElement;
		expect(desktopNav).not.toBeNull();
		expect(desktopNav).toHaveClass(
			"sticky",
			"top-0",
			"self-start",
			"border-r",
			"border-border/40",
		);
	});

	it("navigates to the matching settings route when a tab is selected", async () => {
		Object.defineProperty(window, "innerWidth", {
			configurable: true,
			value: 1440,
			writable: true,
		});

		render(<AdminSettingsPage section="auth" />);

		const storageTab = await screen.findByRole("button", {
			name: /settings_category_storage/i,
		});

		fireEvent.click(storageTab);

		expect(mockState.navigate).toHaveBeenCalledWith("/admin/settings/storage", {
			viewTransition: false,
		});
	});

	it("redirects invalid sections to the first available settings tab", async () => {
		render(<AdminSettingsPage section={"invalid" as never} />);

		await screen.findByRole("button", { name: /settings_category_auth/i });

		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/admin/settings/auth", {
				replace: true,
			});
		});
	});

	it("renders site settings before the other categories when available", async () => {
		Object.defineProperty(window, "innerWidth", {
			configurable: true,
			value: 1440,
			writable: true,
		});

		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general",
					key: "branding_title",
					value: "AsterDrive",
					value_type: "string",
				}),
				createConfig({
					category: "auth",
					key: "auth_access_token_ttl_secs",
					value: "1200",
					value_type: "number",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general",
				key: "branding_title",
				value_type: "string",
			}),
			createSchemaItem({
				category: "auth",
				key: "auth_access_token_ttl_secs",
				value_type: "number",
			}),
		]);

		const { container } = render(<AdminSettingsPage section="general" />);

		await screen.findByDisplayValue("AsterDrive");

		const categoryButtons = Array.from(
			container.querySelectorAll("button[data-value]"),
		).map((button) => button.getAttribute("data-value"));

		expect(categoryButtons.slice(0, 2)).toEqual(["general", "auth"]);
	});

	it("edits non-boolean values inline and saves them with the shared save button", async () => {
		render(<AdminSettingsPage />);

		await screen.findByDisplayValue("20");

		fireEvent.change(screen.getByDisplayValue("20"), {
			target: { value: "30" },
		});

		expect(
			(await screen.findAllByText("settings_save_notice:1")).length,
		).toBeGreaterThan(0);
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"auth_access_token_ttl_secs",
				"1800",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("settings_saved");
	});

	it("shows a CORS validation warning and blocks saving before any request is sent", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "network",
					key: "cors_allowed_origins",
					value: "*",
					value_type: "string",
				}),
				createConfig({
					category: "network",
					key: "cors_allow_credentials",
					value: "false",
					value_type: "boolean",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "network",
				key: "cors_allowed_origins",
				value_type: "string",
			}),
			createSchemaItem({
				category: "network",
				key: "cors_allow_credentials",
				value_type: "boolean",
			}),
		]);

		render(<AdminSettingsPage section="network" />);

		const credentialsSwitch = await screen.findByLabelText(
			"switch:cors_allow_credentials:false",
		);
		fireEvent.click(credentialsSwitch);

		expect(
			await screen.findAllByText("cors_wildcard_credentials_validation_error"),
		).toHaveLength(3);
		expect(mockState.setConfig).not.toHaveBeenCalled();
		expect(screen.getByRole("button", { name: "save_changes" })).toBeDisabled();
	});

	it("allows saving again after the CORS draft is repaired with an explicit origin list", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "network",
					key: "cors_allowed_origins",
					value: "*",
					value_type: "string",
				}),
				createConfig({
					category: "network",
					key: "cors_allow_credentials",
					value: "false",
					value_type: "boolean",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "network",
				key: "cors_allowed_origins",
				value_type: "string",
			}),
			createSchemaItem({
				category: "network",
				key: "cors_allow_credentials",
				value_type: "boolean",
			}),
		]);

		render(<AdminSettingsPage section="network" />);

		fireEvent.click(
			await screen.findByLabelText("switch:cors_allow_credentials:false"),
		);
		expect(
			await screen.findAllByText("cors_wildcard_credentials_validation_error"),
		).toHaveLength(3);

		fireEvent.change(screen.getByDisplayValue("*"), {
			target: { value: "https://panel.example.com" },
		});

		await waitFor(() => {
			expect(
				screen.queryByText("cors_wildcard_credentials_validation_error"),
			).not.toBeInTheDocument();
		});

		const saveButton = screen.getByRole("button", { name: "save_changes" });
		expect(saveButton).not.toBeDisabled();

		fireEvent.click(saveButton);

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"cors_allow_credentials",
				"true",
			);
		});
		expect(mockState.setConfig).toHaveBeenCalledWith(
			"cors_allowed_origins",
			"https://panel.example.com",
		);
	});

	it("renders a friendly time unit selector while keeping raw values on save", async () => {
		render(<AdminSettingsPage section="auth" />);

		expect(await screen.findByDisplayValue("20")).toBeInTheDocument();

		const unitSelect = screen.getByRole("combobox");
		expect(unitSelect).toHaveValue("minutes");

		fireEvent.change(unitSelect, {
			target: { value: "seconds" },
		});
		expect(screen.getByDisplayValue("1200")).toBeInTheDocument();

		fireEvent.change(screen.getByDisplayValue("1200"), {
			target: { value: "1800" },
		});
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"auth_access_token_ttl_secs",
				"1800",
			);
		});
	});

	it("does not reformat scaled number input while it is focused", async () => {
		render(<AdminSettingsPage section="auth" />);

		const ttlInput = (await screen.findByDisplayValue(
			"20",
		)) as HTMLInputElement;
		ttlInput.focus();

		fireEvent.change(ttlInput, {
			target: { value: "21" },
		});

		expect(ttlInput).toHaveValue(21);
		expect(screen.queryByDisplayValue("1260")).not.toBeInTheDocument();

		fireEvent.blur(ttlInput);

		expect(screen.getByDisplayValue("21")).toBeInTheDocument();
	});

	it("ignores invalid scaled number edits and saves blank scaled values explicitly", async () => {
		render(<AdminSettingsPage section="auth" />);

		const ttlInput = (await screen.findByDisplayValue(
			"20",
		)) as HTMLInputElement;

		fireEvent.change(ttlInput, {
			target: { value: "1.5" },
		});
		expect(
			screen.queryByRole("button", { name: "save_changes" }),
		).not.toBeInTheDocument();

		fireEvent.change(ttlInput, {
			target: { value: String(Number.MAX_SAFE_INTEGER + 1) },
		});
		expect(
			screen.queryByRole("button", { name: "save_changes" }),
		).not.toBeInTheDocument();

		fireEvent.change(ttlInput, {
			target: { value: "" },
		});
		fireEvent.click(
			await screen.findByRole("button", { name: "save_changes" }),
		);

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"auth_access_token_ttl_secs",
				"",
			);
		});
	});

	it("does not render a scaled number input when the stored numeric draft is invalid", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "auth",
					key: "auth_access_token_ttl_secs",
					value: "not-a-number",
					value_type: "number",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "auth",
				key: "auth_access_token_ttl_secs",
				value_type: "number",
			}),
		]);

		render(<AdminSettingsPage section="auth" />);

		await screen.findByText("auth_access_token_ttl_secs");
		expect(
			screen.queryByPlaceholderText("config_value"),
		).not.toBeInTheDocument();
	});

	it("ignores scaled size edits that would overflow the stored byte value", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "storage",
					key: "avatar_max_upload_size_bytes",
					value: String(1024 * 1024),
					value_type: "number",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "storage",
				key: "avatar_max_upload_size_bytes",
				value_type: "number",
			}),
		]);

		render(<AdminSettingsPage section="storage" />);

		fireEvent.change(await screen.findByDisplayValue("1"), {
			target: { value: String(Number.MAX_SAFE_INTEGER) },
		});

		expect(
			screen.queryByRole("button", { name: "save_changes" }),
		).not.toBeInTheDocument();
	});

	it("renders a friendly size unit selector while keeping raw byte values on save", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "storage",
					key: "avatar_max_upload_size_bytes",
					value: String(1024 * 1024),
					value_type: "number",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "storage",
				key: "avatar_max_upload_size_bytes",
				value_type: "number",
			}),
		]);

		render(<AdminSettingsPage section="storage" />);

		expect(await screen.findByDisplayValue("1")).toBeInTheDocument();

		const unitSelect = screen.getByRole("combobox");
		expect(unitSelect).toHaveValue("megabytes");

		fireEvent.change(screen.getByDisplayValue("1"), {
			target: { value: "2" },
		});
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"avatar_max_upload_size_bytes",
				String(2 * 1024 * 1024),
			);
		});
	});

	it("renders multiline config values in a textarea and saves the edited template body", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "mail.config",
					key: "mail_smtp_host",
					value: "smtp.example.com",
					value_type: "string",
				}),
				createConfig({
					category: "mail.template",
					key: "mail_template_password_reset_html",
					value: "<p>old body</p>",
					value_type: "multiline",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "mail.config",
				key: "mail_smtp_host",
				value_type: "string",
			}),
			createSchemaItem({
				category: "mail.template",
				key: "mail_template_password_reset_html",
				value_type: "multiline",
			}),
		]);

		render(<AdminSettingsPage section="mail" />);

		expect(await screen.findByText("Config")).toBeInTheDocument();
		expect(screen.getByText("Template")).toBeInTheDocument();
		expect(
			screen.queryByDisplayValue("<p>old body</p>"),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /password reset/i }));

		const textarea = await screen.findByDisplayValue("<p>old body</p>");
		expect(textarea.tagName).toBe("TEXTAREA");
		expect(mockState.codeEditorProps).toMatchObject({
			options: expect.objectContaining({
				wordWrap: "off",
			}),
			value: "<p>old body</p>",
		});

		fireEvent.change(textarea, {
			target: { value: "<p>new body</p>" },
		});

		expect(
			(await screen.findAllByText("settings_save_notice:1")).length,
		).toBeGreaterThan(0);
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"mail_template_password_reset_html",
				"<p>new body</p>",
			);
		});
	});

	it("opens the template variables dialog from the matching html template field", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "mail.template",
					key: "mail_template_password_reset_html",
					value: "<p>Reset</p>",
					value_type: "multiline",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "mail.template",
				key: "mail_template_password_reset_html",
				value_type: "multiline",
			}),
		]);

		render(<AdminSettingsPage section="mail" />);

		await screen.findByText("Template");
		fireEvent.click(
			await screen.findByRole("button", { name: /password reset/i }),
		);
		fireEvent.click(
			await screen.findByRole("button", {
				name: "mail_template_variable_link",
			}),
		);

		expect(
			await screen.findByText("mail_template_variables_dialog_title"),
		).toBeInTheDocument();
		expect((await screen.findAllByText("{{username}}")).length).toBeGreaterThan(
			0,
		);
		expect(
			(await screen.findAllByText("{{reset_url}}")).length,
		).toBeGreaterThan(0);
		expect(
			(await screen.findAllByText("{{site_name}}")).length,
		).toBeGreaterThan(0);
	});

	it("renders external sign-in email verification templates with i18n and variables", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "mail.template",
					description:
						"Subject template for external auth email verification messages.",
					key: "mail_template_external_auth_email_verification_subject",
					value: "Verify your email for AsterDrive sign-in",
					value_type: "string",
				}),
				createConfig({
					category: "mail.template",
					description:
						"HTML template for external auth email verification messages. A complete HTML document is recommended for best client compatibility.",
					key: "mail_template_external_auth_email_verification_html",
					value: "<p>Verify {{email}}</p>",
					value_type: "multiline",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "mail.template",
				description:
					"Subject template for external auth email verification messages.",
				description_i18n_key:
					"settings_item_mail_template_external_auth_email_verification_subject_desc",
				key: "mail_template_external_auth_email_verification_subject",
				label_i18n_key:
					"settings_item_mail_template_external_auth_email_verification_subject_label",
				value_type: "string",
			}),
			createSchemaItem({
				category: "mail.template",
				description:
					"HTML template for external auth email verification messages. A complete HTML document is recommended for best client compatibility.",
				description_i18n_key:
					"settings_item_mail_template_external_auth_email_verification_html_desc",
				key: "mail_template_external_auth_email_verification_html",
				label_i18n_key:
					"settings_item_mail_template_external_auth_email_verification_html_label",
				value_type: "multiline",
			}),
		]);
		mockState.templateVariables.mockResolvedValueOnce([
			createTemplateVariableGroup({
				label_i18n_key:
					"settings_mail_template_group_external_auth_email_verification",
				template_code: "external_auth_email_verification",
				variables: [
					{
						description_i18n_key: "settings_template_variable_email_desc",
						label_i18n_key: "settings_template_variable_email_label",
						token: "{{email}}",
					},
					{
						description_i18n_key:
							"settings_template_variable_verification_url_desc",
						label_i18n_key: "settings_template_variable_verification_url_label",
						token: "{{verification_url}}",
					},
					{
						description_i18n_key:
							"settings_template_variable_provider_name_desc",
						label_i18n_key: "settings_template_variable_provider_name_label",
						token: "{{provider_name}}",
					},
					{
						description_i18n_key: "settings_template_variable_site_name_desc",
						label_i18n_key: "settings_template_variable_site_name_label",
						token: "{{site_name}}",
					},
					{
						description_i18n_key: "settings_template_variable_expires_in_desc",
						label_i18n_key: "settings_template_variable_expires_in_label",
						token: "{{expires_in}}",
					},
				],
			}),
		]);

		render(<AdminSettingsPage section="mail" />);

		await screen.findByText("Template");
		fireEvent.click(
			await screen.findByRole("button", {
				name: /external sign-in email verification/i,
			}),
		);

		expect(
			screen.getByText("External sign-in email verification subject"),
		).toBeInTheDocument();
		expect(
			screen.getByText(
				"Subject template for external sign-in email verification emails.",
			),
		).toBeInTheDocument();
		expect(
			screen.getByText("External sign-in email verification HTML body"),
		).toBeInTheDocument();
		expect(
			screen.getByText(
				"HTML template for external sign-in email verification emails. A complete HTML document is recommended for best client compatibility.",
			),
		).toBeInTheDocument();
		expect(
			screen.queryByText(/OIDC email verification/i),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText("mail_template_external_auth_email_verification_html"),
		).not.toBeInTheDocument();

		fireEvent.click(
			await screen.findByRole("button", {
				name: "mail_template_variable_link",
			}),
		);

		expect(
			await screen.findByText("mail_template_variables_dialog_title"),
		).toBeInTheDocument();
		expect(screen.getByText("Email")).toBeInTheDocument();
		expect(screen.getByText("Verification URL")).toBeInTheDocument();
		expect(screen.getByText("Provider Name")).toBeInTheDocument();
		expect(screen.getByText("Site Name")).toBeInTheDocument();
		expect(screen.getByText("Expires In")).toBeInTheDocument();
		expect((await screen.findAllByText("{{email}}")).length).toBeGreaterThan(0);
		expect(
			(await screen.findAllByText("{{verification_url}}")).length,
		).toBeGreaterThan(0);
		expect(
			(await screen.findAllByText("{{provider_name}}")).length,
		).toBeGreaterThan(0);
		expect(
			(await screen.findAllByText("{{site_name}}")).length,
		).toBeGreaterThan(0);
		expect(
			(await screen.findAllByText("{{expires_in}}")).length,
		).toBeGreaterThan(0);
		expect(screen.queryByText("{{username}}")).not.toBeInTheDocument();
	});

	it("saves staged changes when Cmd+S is pressed from a focused input", async () => {
		render(<AdminSettingsPage />);

		const ttlInput = await screen.findByDisplayValue("20");
		ttlInput.focus();

		fireEvent.change(ttlInput, {
			target: { value: "30" },
		});

		expect(
			await screen.findByText("settings_save_notice:1"),
		).toBeInTheDocument();

		const saveEvent = createEvent.keyDown(ttlInput, {
			bubbles: true,
			cancelable: true,
			key: "s",
			metaKey: true,
		});

		fireEvent(ttlInput, saveEvent);

		expect(saveEvent.defaultPrevented).toBe(true);

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"auth_access_token_ttl_secs",
				"1800",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("settings_saved");
	});

	it("shows the combined save hint when valid staged changes are present", async () => {
		render(<AdminSettingsPage />);

		await screen.findByDisplayValue("20");

		fireEvent.change(screen.getByDisplayValue("20"), {
			target: { value: "30" },
		});

		expect(
			await screen.findByText(
				"更改会先暂存为草稿，确认无误后再统一保存，⌘/Ctrl + S 保存。",
			),
		).toBeInTheDocument();
	});

	it("renders the save actions in a floating bottom bar", async () => {
		render(<AdminSettingsPage />);

		await screen.findByDisplayValue("20");

		fireEvent.change(screen.getByDisplayValue("20"), {
			target: { value: "30" },
		});

		const saveBar = await screen.findByTestId("settings-save-bar");

		expect(saveBar).toHaveClass("fixed", "bottom-0", "pointer-events-none");
		expect(saveBar).toHaveAttribute("aria-hidden", "false");
		expect(saveBar).toHaveTextContent("settings_save_notice:1");
	});

	it("reserves bottom space for settings content while the floating save bar is visible", async () => {
		render(<AdminSettingsPage />);

		await screen.findByDisplayValue("20");

		fireEvent.change(screen.getByDisplayValue("20"), {
			target: { value: "30" },
		});

		await screen.findByTestId("settings-save-bar");

		await waitFor(() => {
			expect(screen.getByTestId("settings-content")).toHaveStyle({
				paddingBottom: "160px",
			});
		});
	});

	it("keeps the floating save bar mounted for the exit animation before unmounting", async () => {
		render(<AdminSettingsPage />);

		await screen.findByDisplayValue("20");

		fireEvent.change(screen.getByDisplayValue("20"), {
			target: { value: "30" },
		});

		const saveBar = await screen.findByTestId("settings-save-bar");

		vi.useFakeTimers();
		fireEvent.click(screen.getByRole("button", { name: "undo_changes" }));

		expect(saveBar).toBeInTheDocument();
		expect(saveBar).toHaveAttribute("aria-hidden", "true");

		await act(async () => {
			await vi.advanceTimersByTimeAsync(140);
		});

		expect(screen.queryByTestId("settings-save-bar")).not.toBeInTheDocument();
		vi.useRealTimers();
	});

	it("renders translated system config metadata without exposing the raw config key", async () => {
		render(<AdminSettingsPage section="auth" />);

		await screen.findByDisplayValue("20");

		expect(screen.getByText("Access token lifetime")).toBeInTheDocument();
		expect(
			screen.getByText(
				"Controls how long newly issued access tokens stay valid.",
			),
		).toBeInTheDocument();
		expect(
			screen.queryByText("auth_access_token_ttl_secs"),
		).not.toBeInTheDocument();
		expect(screen.queryByText("ttl desc")).not.toBeInTheDocument();
	});

	it("shows a favicon asset preview next to the branding URL field", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general",
					description: "favicon desc",
					key: "branding_favicon_url",
					value: "/branding/favicon.svg",
					value_type: "string",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general",
				description: "favicon desc",
				key: "branding_favicon_url",
				value_type: "string",
			}),
		]);

		render(<AdminSettingsPage section="general" />);

		expect(
			await screen.findByDisplayValue("/branding/favicon.svg"),
		).toBeInTheDocument();
		expect(screen.getByTestId("branding-asset-preview")).toBeInTheDocument();
		expect(screen.getByTestId("branding-asset-preview-image")).toHaveAttribute(
			"src",
			"/branding/favicon.svg",
		);
		expect(screen.getByLabelText("/branding/favicon.svg")).toBeInTheDocument();
		expect(screen.getByLabelText("/branding/favicon.svg")).toHaveClass(
			"bg-white",
			"w-12",
		);
	});

	it("uses a light wider preview frame for the light-surface wordmark field", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general",
					description: "wordmark desc",
					key: "branding_wordmark_dark_url",
					value: "/branding/wordmark-dark.svg",
					value_type: "string",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general",
				description: "wordmark desc",
				key: "branding_wordmark_dark_url",
				value_type: "string",
			}),
		]);

		render(<AdminSettingsPage section="general" />);

		expect(
			await screen.findByDisplayValue("/branding/wordmark-dark.svg"),
		).toBeInTheDocument();
		expect(screen.getByTestId("branding-asset-preview")).toBeInTheDocument();
		expect(screen.getByTestId("branding-asset-preview-image")).toHaveAttribute(
			"src",
			"/branding/wordmark-dark.svg",
		);
		expect(
			screen.getByLabelText("/branding/wordmark-dark.svg"),
		).toBeInTheDocument();
		expect(screen.getByLabelText("/branding/wordmark-dark.svg")).toHaveClass(
			"bg-white",
			"w-36",
		);
	});

	it("uses a dark wider preview frame for the dark-surface wordmark field", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general",
					key: "branding_wordmark_light_url",
					value: "/branding/wordmark-light.svg",
					value_type: "string",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general",
				key: "branding_wordmark_light_url",
				value_type: "string",
			}),
		]);

		render(<AdminSettingsPage section="general" />);

		const preview = await screen.findByLabelText(
			"/branding/wordmark-light.svg",
		);
		expect(preview).toHaveClass("bg-black", "w-36");
	});

	it("renders preview app registry config with the structured list editor", async () => {
		mockState.codeEditorProps = null;
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general.preview",
					key: "frontend_preview_apps_json",
					value: JSON.stringify(
						{
							version: 2,
							apps: [
								{
									extensions: ["png", "jpg", "jpeg", "gif"],
									key: "builtin.image",
									icon: "Eye",
									enabled: true,
									labels: {
										en: "Image preview",
										zh: "图片预览",
									},
									provider: "builtin",
								},
							],
						},
						null,
						2,
					),
					value_type: "multiline",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general.preview",
				key: "frontend_preview_apps_json",
				value_type: "multiline",
			}),
		]);

		render(<AdminSettingsPage section="general" />);

		fireEvent.click(
			await screen.findByRole("button", { name: /settings_section_expand/i }),
		);

		expect(
			await screen.findByText("preview_apps_editor_title"),
		).toBeInTheDocument();
		expect(
			screen.getAllByText((content) =>
				["Image preview", "图片预览", "builtin.image"].includes(content),
			).length,
		).toBeGreaterThan(0);
		expect(screen.queryByLabelText("Code editor")).not.toBeInTheDocument();
		expect(mockState.codeEditorProps).toBeNull();
	});

	it("renders media metadata configs in the media processing storage subcategory", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "storage.media_processing",
					key: "media_metadata_enabled",
					value: "true",
					value_type: "boolean",
				}),
				createConfig({
					category: "storage.media_processing",
					key: "media_metadata_max_source_bytes",
					value: "1073741824",
					value_type: "number",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "storage.media_processing",
				description_i18n_key: "settings_item_media_metadata_enabled_desc",
				key: "media_metadata_enabled",
				label_i18n_key: "settings_item_media_metadata_enabled_label",
				value_type: "boolean",
			}),
			createSchemaItem({
				category: "storage.media_processing",
				key: "media_metadata_max_source_bytes",
				label_i18n_key: "settings_item_media_metadata_max_source_bytes_label",
				value_type: "number",
			}),
		]);

		render(<AdminSettingsPage section="storage" />);

		expect(await screen.findByText("Media Processing")).toBeInTheDocument();
		expect(
			screen.getByText(
				"Configure media metadata, available thumbnail processors, and safety limits.",
			),
		).toBeInTheDocument();
		expect(
			await screen.findByText("Enable media metadata"),
		).toBeInTheDocument();
		expect(
			screen.getByText(
				"Allow the backend to parse and cache basic image, audio, and video metadata by blob.",
			),
		).toBeInTheDocument();
		expect(
			screen.getByText("Media metadata source-size cap"),
		).toBeInTheDocument();
		expect(screen.queryByLabelText("Code editor")).not.toBeInTheDocument();
	});

	it("renders media processing registry with the custom editor", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "storage.media_processing",
					key: "media_processing_registry_json",
					value: JSON.stringify(
						{
							version: 2,
							processors: [
								{
									kind: "vips_cli",
									enabled: true,
									extensions: ["heic"],
									uses: ["thumbnail:image"],
									config: {
										command: "custom-vips",
									},
								},
								{
									kind: "images",
									enabled: true,
									uses: ["thumbnail:image", "metadata:image"],
								},
							],
						},
						null,
						2,
					),
					value_type: "multiline",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "storage.media_processing",
				key: "media_processing_registry_json",
				label_i18n_key: "settings_item_media_processing_registry_json_label",
				value_type: "multiline",
			}),
		]);

		render(<AdminSettingsPage section="storage" />);

		fireEvent.click(
			await screen.findByRole("button", { name: /settings_section_expand/i }),
		);
		expect(await screen.findByText("AsterDrive Built-in")).toBeInTheDocument();
		expect(screen.getByDisplayValue("custom-vips")).toBeInTheDocument();
		expect(
			screen.getAllByRole("button", {
				name: /switch:media-processing-.*-enabled:/,
			}).length,
		).toBeGreaterThan(0);

		expect(screen.queryByLabelText("Code editor")).not.toBeInTheDocument();
	});

	it("tests the vips command against the current media processing draft", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "storage.media_processing",
					key: "media_processing_registry_json",
					value: JSON.stringify(
						{
							version: 2,
							processors: [
								{
									kind: "vips_cli",
									enabled: false,
									extensions: ["heic"],
									uses: ["thumbnail:image"],
									config: {
										command: "/usr/local/bin/vips-custom",
									},
								},
								{
									kind: "images",
									enabled: true,
									uses: ["thumbnail:image", "metadata:image"],
								},
							],
						},
						null,
						2,
					),
					value_type: "multiline",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "storage.media_processing",
				key: "media_processing_registry_json",
				label_i18n_key: "settings_item_media_processing_registry_json_label",
				value_type: "multiline",
			}),
		]);
		mockState.actionConfig.mockResolvedValueOnce({
			message:
				"vips_cli command '/usr/local/bin/vips-custom' is available: vips-8.16.0",
		});

		render(<AdminSettingsPage section="storage" />);

		fireEvent.click(
			await screen.findByRole("button", { name: /settings_section_expand/i }),
		);
		const vipsCard = (await screen.findByText(/vips_cli/i)).closest(
			"[data-slot='card']",
		);
		expect(vipsCard).not.toBeNull();
		fireEvent.click(
			within(vipsCard as HTMLElement).getByRole("button", {
				name: /media_processing_editor_processor_test_command/i,
			}),
		);

		await waitFor(() => {
			expect(mockState.actionConfig).toHaveBeenCalledWith(
				"media_processing_registry_json",
				expect.objectContaining({
					action: "test_vips_cli",
					value: expect.stringContaining("/usr/local/bin/vips-custom"),
				}),
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"vips_cli command '/usr/local/bin/vips-custom' is available: vips-8.16.0",
		);
	});

	it("tests the ffprobe command against the current media processing draft", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "storage.media_processing",
					key: "media_processing_registry_json",
					value: JSON.stringify(
						{
							version: 2,
							processors: [
								{
									kind: "ffprobe_cli",
									enabled: false,
									extensions: ["mp4"],
									uses: ["metadata:video"],
									config: {
										command: "/opt/bin/ffprobe-custom",
									},
								},
								{
									kind: "images",
									enabled: true,
									uses: ["thumbnail:image", "metadata:image"],
								},
							],
						},
						null,
						2,
					),
					value_type: "multiline",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "storage.media_processing",
				key: "media_processing_registry_json",
				label_i18n_key: "settings_item_media_processing_registry_json_label",
				value_type: "multiline",
			}),
		]);
		mockState.actionConfig.mockResolvedValueOnce({
			message: "ffprobe command '/opt/bin/ffprobe-custom' is available",
		});

		render(<AdminSettingsPage section="storage" />);

		fireEvent.click(
			await screen.findByRole("button", { name: /settings_section_expand/i }),
		);
		fireEvent.click(
			(
				await screen.findAllByRole("button", {
					name: /media_processing_editor_processor_test_command/i,
				})
			).find((button) =>
				button
					.closest("[data-slot='card']")
					?.textContent?.includes("ffprobe_cli"),
			) ??
				screen.getAllByRole("button", {
					name: /media_processing_editor_processor_test_command/i,
				})[0],
		);

		await waitFor(() => {
			expect(mockState.actionConfig).toHaveBeenCalledWith(
				"media_processing_registry_json",
				expect.objectContaining({
					action: "test_ffprobe_cli",
					value: expect.stringContaining("/opt/bin/ffprobe-custom"),
				}),
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"ffprobe command '/opt/bin/ffprobe-custom' is available",
		);
	});

	it("defaults archive storage subcategory sections to collapsed", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "storage.archive_extract",
					key: "archive_extract_max_entries",
					value: "10000",
					value_type: "number",
				}),
				createConfig({
					category: "storage.archive_preview",
					key: "archive_preview_max_entries",
					value: "2000",
					value_type: "number",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "storage.archive_extract",
				key: "archive_extract_max_entries",
				value_type: "number",
			}),
			createSchemaItem({
				category: "storage.archive_preview",
				key: "archive_preview_max_entries",
				value_type: "number",
			}),
		]);

		render(<AdminSettingsPage section="storage" />);

		const extractTitle = await screen.findByText("Archive Extraction");
		const previewTitle = await screen.findByText("Archive Preview");
		expect(screen.queryByText("archive_extract_max_entries")).toBeNull();
		expect(screen.queryByText("archive_preview_max_entries")).toBeNull();

		const extractSection = extractTitle.closest("section") as HTMLElement;
		fireEvent.click(
			within(extractSection).getByRole("button", {
				name: /settings_section_expand/i,
			}),
		);

		expect(
			await screen.findByText("archive_extract_max_entries"),
		).toBeInTheDocument();
		expect(screen.queryByText("archive_preview_max_entries")).toBeNull();
		expect(previewTitle).toBeInTheDocument();
	});

	it("builds WOPI discovery apps into the local app registry draft", async () => {
		const initialPreviewValue = JSON.stringify(
			{
				version: 2,
				apps: [
					{
						key: "custom.viewer",
						provider: "url_template",
						icon: "https://viewer.example.com/icon.svg",
						enabled: true,
						extensions: ["md"],
						labels: {
							en: "Viewer",
						},
						config: {
							mode: "iframe",
							url_template:
								"https://viewer.example.com/embed?src={{file_preview_url}}",
							allowed_origins: ["https://viewer.example.com"],
						},
					},
				],
			},
			null,
			2,
		);

		mockState.actionConfig.mockResolvedValueOnce({
			message: "Built preview apps from discovery",
			value: JSON.stringify(
				{
					version: 2,
					apps: [
						{
							key: "custom.viewer",
							provider: "url_template",
							icon: "https://viewer.example.com/icon.svg",
							enabled: true,
							extensions: ["md"],
							labels: {
								en: "Viewer",
							},
							config: {
								mode: "iframe",
								url_template:
									"https://viewer.example.com/embed?src={{file_preview_url}}",
								allowed_origins: ["https://viewer.example.com"],
							},
						},
						{
							key: "custom.wopi.localhost.8080.hosting.discovery__wopi_discovery__word",
							provider: "wopi",
							icon: "http://localhost:8080/word.ico",
							enabled: true,
							extensions: ["doc", "docx"],
							labels: {
								en: "Word",
							},
							config: {
								mode: "iframe",
								action: "view",
								discovery_url: "http://localhost:8080/hosting/discovery",
							},
						},
					],
				},
				null,
				2,
			),
		});
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general.preview",
					key: "frontend_preview_apps_json",
					value: initialPreviewValue,
					value_type: "multiline",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general.preview",
				key: "frontend_preview_apps_json",
				value_type: "multiline",
			}),
		]);

		render(<AdminSettingsPage section="general" />);

		fireEvent.click(
			await screen.findByRole("button", { name: /settings_section_expand/i }),
		);
		await screen.findByText("preview_apps_editor_title");

		fireEvent.click(
			screen.getByRole("button", { name: /preview_apps_add_app/ }),
		);
		fireEvent.click(
			screen
				.getByText("preview_apps_add_dialog_wopi_title")
				.closest("button") as HTMLButtonElement,
		);
		fireEvent.change(
			screen.getByLabelText("preview_apps_wopi_discovery_dialog_label"),
			{
				target: { value: "http://localhost:8080/hosting/discovery" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "preview_apps_wopi_discovery_dialog_submit",
			}),
		);

		await waitFor(() => {
			expect(mockState.actionConfig).toHaveBeenCalledWith(
				"frontend_preview_apps_json",
				{
					action: "build_wopi_discovery_preview_config",
					discovery_url: "http://localhost:8080/hosting/discovery",
					value: initialPreviewValue,
				},
			);
		});
		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"preview_apps_wopi_discovery_success",
			);
		});
		expect(screen.getAllByText("Word").length).toBeGreaterThan(0);
	});

	it("debounces favicon asset preview updates while typing", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general",
					key: "branding_favicon_url",
					value: "/branding/favicon.svg",
					value_type: "string",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general",
				key: "branding_favicon_url",
				value_type: "string",
			}),
		]);

		render(<AdminSettingsPage section="general" />);

		const input = await screen.findByDisplayValue("/branding/favicon.svg");
		expect(screen.getByLabelText("/branding/favicon.svg")).toBeInTheDocument();

		vi.useFakeTimers();
		fireEvent.change(input, {
			target: { value: "/branding/next.svg" },
		});

		expect(
			screen.queryByLabelText("/branding/next.svg"),
		).not.toBeInTheDocument();
		expect(screen.getByLabelText("/branding/favicon.svg")).toBeInTheDocument();

		await act(async () => {
			await vi.advanceTimersByTimeAsync(300);
		});

		expect(screen.getByLabelText("/branding/next.svg")).toBeInTheDocument();
	});

	it("edits public site URL origins as stable rows and saves the resulting array", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general",
					key: "public_site_url",
					value: ["https://drive.example.com"],
					value_type: "string_array",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general",
				key: "public_site_url",
				label_i18n_key: "settings_item_public_site_url_label",
				value_type: "string_array",
			}),
		]);

		render(<AdminSettingsPage section="general" />);

		const firstOrigin = await screen.findByLabelText(
			"public_site_url_origin_label 1",
		);
		expect(firstOrigin).toHaveValue("https://drive.example.com");
		expect(
			screen.getByText("public_site_url_primary_origin"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "public_site_url_add_origin" }),
		);
		fireEvent.change(screen.getByLabelText("public_site_url_origin_label 2"), {
			target: { value: "https://panel.example.com" },
		});
		expect(screen.getByLabelText("public_site_url_origin_label 1")).toHaveValue(
			"https://drive.example.com",
		);

		fireEvent.click(
			screen.getAllByRole("button", {
				name: "public_site_url_remove_origin",
			})[0],
		);
		expect(screen.getByLabelText("public_site_url_origin_label 1")).toHaveValue(
			"https://panel.example.com",
		);

		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith("public_site_url", [
				"https://panel.example.com",
			]);
		});
	});

	it("can append the current browser origin to the public site URL draft", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general",
					key: "public_site_url",
					value: ["http://localhost:3000", "http://localhost:5173"],
					value_type: "string_array",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general",
				key: "public_site_url",
				label_i18n_key: "settings_item_public_site_url_label",
				value_type: "string_array",
			}),
		]);
		Object.defineProperty(window, "location", {
			configurable: true,
			value: {
				...window.location,
				origin: "http://localhost:5174",
			},
		});

		render(<AdminSettingsPage section="general" />);

		await screen.findByLabelText("public_site_url_origin_label 1");
		fireEvent.click(
			screen.getByRole("button", {
				name: "public_site_url_add_current_origin",
			}),
		);

		expect(screen.getByLabelText("public_site_url_origin_label 3")).toHaveValue(
			"http://localhost:5174",
		);

		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith("public_site_url", [
				"http://localhost:3000",
				"http://localhost:5173",
				"http://localhost:5174",
			]);
		});
	});

	it("does not show the current browser origin shortcut when it is already configured", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general",
					key: "public_site_url",
					value: [window.location.origin],
					value_type: "string_array",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general",
				key: "public_site_url",
				label_i18n_key: "settings_item_public_site_url_label",
				value_type: "string_array",
			}),
		]);

		render(<AdminSettingsPage section="general" />);

		await screen.findByLabelText("public_site_url_origin_label 1");

		expect(
			screen.queryByRole("button", {
				name: "public_site_url_add_current_origin",
			}),
		).not.toBeInTheDocument();
	});

	it("edits generic string-array configs with list rows and saves arrays", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "webdav",
					key: "webdav_block_system_file_patterns",
					value: [".DS_Store"],
					value_type: "string_array",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "webdav",
				key: "webdav_block_system_file_patterns",
				label_i18n_key: "settings_item_webdav_block_system_file_patterns_label",
				value_type: "string_array",
			}),
		]);

		render(<AdminSettingsPage section="webdav" />);

		const firstPattern = await screen.findByLabelText(
			"settings_string_array_item_label 1",
		);
		expect(firstPattern).toHaveValue(".DS_Store");

		fireEvent.click(
			screen.getByRole("button", { name: "settings_string_array_add_item" }),
		);
		fireEvent.change(
			screen.getByLabelText("settings_string_array_item_label 2"),
			{
				target: { value: "Thumbs.db" },
			},
		);

		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"webdav_block_system_file_patterns",
				[".DS_Store", "Thumbs.db"],
			);
		});
	});

	it("clears the public site URL draft when the only origin row is removed", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: [
				createConfig({
					category: "general",
					key: "public_site_url",
					value: ["https://drive.example.com"],
					value_type: "string_array",
				}),
			],
		});
		mockState.schema.mockResolvedValueOnce([
			createSchemaItem({
				category: "general",
				key: "public_site_url",
				value_type: "string_array",
			}),
		]);

		render(<AdminSettingsPage section="general" />);

		await screen.findByLabelText("public_site_url_origin_label 1");
		fireEvent.click(
			screen.getByRole("button", { name: "public_site_url_remove_origin" }),
		);

		expect(screen.getByLabelText("public_site_url_origin_label 1")).toHaveValue(
			"",
		);
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith("public_site_url", []);
		});
	});

	it("discards draft changes without sending any requests", async () => {
		render(<AdminSettingsPage />);

		await screen.findByDisplayValue("20");

		fireEvent.change(screen.getByDisplayValue("20"), {
			target: { value: "30" },
		});

		expect(
			await screen.findByText("settings_save_notice:1"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "undo_changes" }));

		await waitFor(() => {
			expect(
				screen.queryByRole("button", { name: "save_changes" }),
			).not.toBeInTheDocument();
		});
		expect(screen.getByDisplayValue("20")).toBeInTheDocument();
		expect(mockState.setConfig).not.toHaveBeenCalled();
		expect(mockState.deleteConfig).not.toHaveBeenCalled();
	});

	it("stages custom config creation and deletion until the shared save button is clicked", async () => {
		render(<AdminSettingsPage section="custom" />);

		await screen.findByDisplayValue("ocean");

		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));
		fireEvent.click(screen.getByRole("button", { name: /add_custom_config/i }));

		fireEvent.change(
			screen.getByPlaceholderText("custom_config_key_placeholder"),
			{
				target: { value: "custom.accent" },
			},
		);
		fireEvent.change(
			screen.getAllByPlaceholderText("config_value").slice(-1)[0],
			{
				target: { value: "sunset" },
			},
		);

		expect(
			await screen.findByText("settings_save_notice:2"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.deleteConfig).toHaveBeenCalledWith("custom.theme");
			expect(mockState.setConfig).toHaveBeenCalledWith(
				"custom.accent",
				"sunset",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("settings_saved");
	});
});

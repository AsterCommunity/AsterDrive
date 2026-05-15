import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createContext, useContext } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SettingsPage from "@/pages/SettingsPage";

const mockState = vi.hoisted(() => ({
	authService: {
		changePassword: vi.fn(),
		listPasskeys: vi.fn(),
		listSessions: vi.fn(),
		startPasskeyRegistration: vi.fn(),
		finishPasskeyRegistration: vi.fn(),
		renamePasskey: vi.fn(),
		deletePasskey: vi.fn(),
		revokeOtherSessions: vi.fn(),
		revokeSession: vi.fn(),
		requestEmailChange: vi.fn(),
		resendEmailChange: vi.fn(),
		updateProfile: vi.fn(),
		setAvatarSource: vi.fn(),
		uploadAvatar: vi.fn(),
	},
	authStore: {
		forceLogout: vi.fn(),
		refreshUser: vi.fn(),
		setStorageEventStreamEnabled: vi.fn(),
		syncSession: vi.fn(),
		user: {
			email: "alice@example.com",
			id: 1,
			preferences: {
				storage_event_stream_enabled: true,
			},
			profile: {
				display_name: null,
				avatar: {
					source: "none",
					url_512: null,
					url_1024: null,
					version: 0,
				},
			},
			role: "user",
			status: "active",
			storage_quota: 0,
			storage_used: 0,
			username: "alice",
		},
	},
	changeLanguage: vi.fn(),
	displayTimeZoneStore: {
		preference: "browser",
		setPreference: vi.fn(),
	},
	fileStore: {
		browserOpenMode: "double_click" as "single_click" | "double_click",
		setBrowserOpenMode: vi.fn(),
		setViewMode: vi.fn(),
		viewMode: "list" as "list" | "grid",
	},
	navigate: vi.fn(),
	preferenceSync: vi.fn(),
	themeStore: {
		mode: "dark" as "light" | "dark" | "system",
		setMode: vi.fn(),
	},
	toastSuccess: vi.fn(),
	translationLanguage: "zh-CN",
	webAuthn: {
		createPasskeyCredential: vi.fn(),
		supported: false,
	},
	location: {
		hash: "",
		pathname: "/settings/security",
		search: "",
	},
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		i18n: {
			changeLanguage: mockState.changeLanguage,
			language: mockState.translationLanguage,
		},
		t: (key: string, vars?: Record<string, unknown>) =>
			vars ? `${key}:${JSON.stringify(vars)}` : key,
	}),
}));

vi.mock("react-router-dom", () => ({
	useLocation: () => mockState.location,
	useNavigate: () => mockState.navigate,
	useParams: () => ({}),
}));

vi.mock("@/components/common/ColorPresetPicker", () => ({
	ColorPresetPicker: () => <div>color-preset-picker</div>,
}));

vi.mock("@/components/common/UserAvatarImage", () => ({
	UserAvatarImage: ({ name }: { name: string }) => (
		<div>{`avatar:${name}`}</div>
	),
}));

vi.mock("@/components/settings/AvatarCropDialog", () => ({
	AvatarCropDialog: ({
		file,
		onConfirm,
		onOpenChange,
		open,
	}: {
		file: File | null;
		onConfirm: (file: File) => Promise<boolean>;
		onOpenChange: (open: boolean) => void;
		open: boolean;
	}) =>
		open ? (
			<div data-testid="avatar-crop-dialog">
				<div>{file?.name ?? ""}</div>
				<button
					type="button"
					onClick={() =>
						onConfirm(
							new File(["cropped"], "cropped-avatar.webp", {
								type: "image/webp",
							}),
						)
					}
				>
					settings:settings_avatar_crop_apply
				</button>
				<button type="button" onClick={() => onOpenChange(false)}>
					close-crop-dialog
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/common/SettingsScaffold", () => ({
	SettingsPageIntro: ({
		title,
		description,
	}: {
		title: string;
		description?: string;
	}) => (
		<div>
			<h1>{title}</h1>
			<p>{description}</p>
		</div>
	),
	SettingsSection: ({
		title,
		description,
		children,
	}: {
		title: string;
		description?: string;
		children: React.ReactNode;
	}) => (
		<section>
			<h2>{title}</h2>
			<p>{description}</p>
			{children}
		</section>
	),
	SettingsRow: ({
		label,
		description,
		children,
	}: {
		label: string;
		description?: string;
		children: React.ReactNode;
	}) => (
		<div>
			<div>{label}</div>
			<div>{description}</div>
			{children}
		</div>
	),
	SettingsChoiceGroup: ({
		options,
		value,
		onChange,
	}: {
		options: Array<{ label: string; value: string }>;
		value: string;
		onChange: (value: never) => void;
	}) => (
		<div data-testid="choice-group" data-value={value}>
			{options.map((option) => (
				<button
					key={option.value}
					type="button"
					onClick={() => onChange(option.value as never)}
				>
					{option.label}
				</button>
			))}
		</div>
	),
}));

const SelectContext = createContext<{
	onValueChange?: (value: string) => void;
	value: string;
}>({
	value: "",
});

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		onValueChange,
		value,
	}: {
		children: React.ReactNode;
		onValueChange?: (value: string) => void;
		value: string;
	}) => (
		<SelectContext.Provider value={{ onValueChange, value }}>
			<div data-testid="select" data-value={value}>
				{children}
			</div>
		</SelectContext.Provider>
	),
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectGroup: ({ children }: { children: React.ReactNode }) => (
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
			<button type="button" onClick={() => context.onValueChange?.(value)}>
				{children}
			</button>
		);
	},
	SelectLabel: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectSeparator: () => <hr />,
	SelectTrigger: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectValue: () => {
		const context = useContext(SelectContext);
		return <span>{context.value}</span>;
	},
}));

vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({ children }: { children: React.ReactNode }) => (
		<div data-testid="app-layout">{children}</div>
	),
}));

const TabsContext = createContext<{
	onValueChange?: (value: string) => void;
	value: string;
}>({
	value: "",
});

vi.mock("@/components/ui/tabs", () => ({
	Tabs: ({
		children,
		defaultValue,
		onValueChange,
		value,
	}: {
		children: React.ReactNode;
		defaultValue?: string;
		onValueChange?: (value: string) => void;
		value?: string;
	}) => (
		<TabsContext.Provider
			value={{ onValueChange, value: value ?? defaultValue ?? "" }}
		>
			<div>{children}</div>
		</TabsContext.Provider>
	),
	TabsList: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TabsTrigger: ({
		children,
		value,
	}: {
		children: React.ReactNode;
		value: string;
	}) => {
		const context = useContext(TabsContext);
		return (
			<button
				type="button"
				data-value={value}
				onClick={() => context.onValueChange?.(value)}
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
		value: string;
	}) => {
		const context = useContext(TabsContext);
		return context.value === value ? <div>{children}</div> : null;
	},
}));

vi.mock("@/lib/preferenceSync", () => ({
	queuePreferenceSync: (...args: unknown[]) =>
		mockState.preferenceSync(...args),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: vi.fn(),
}));

vi.mock("sonner", () => ({
	toast: {
		error: vi.fn(),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/services/authService", () => ({
	authService: {
		changePassword: (...args: unknown[]) =>
			mockState.authService.changePassword(...args),
		deletePasskey: (...args: unknown[]) =>
			mockState.authService.deletePasskey(...args),
		finishPasskeyRegistration: (...args: unknown[]) =>
			mockState.authService.finishPasskeyRegistration(...args),
		listPasskeys: (...args: unknown[]) =>
			mockState.authService.listPasskeys(...args),
		listSessions: (...args: unknown[]) =>
			mockState.authService.listSessions(...args),
		renamePasskey: (...args: unknown[]) =>
			mockState.authService.renamePasskey(...args),
		revokeOtherSessions: (...args: unknown[]) =>
			mockState.authService.revokeOtherSessions(...args),
		revokeSession: (...args: unknown[]) =>
			mockState.authService.revokeSession(...args),
		requestEmailChange: (...args: unknown[]) =>
			mockState.authService.requestEmailChange(...args),
		resendEmailChange: (...args: unknown[]) =>
			mockState.authService.resendEmailChange(...args),
		updateProfile: (...args: unknown[]) =>
			mockState.authService.updateProfile(...args),
		setAvatarSource: (...args: unknown[]) =>
			mockState.authService.setAvatarSource(...args),
		startPasskeyRegistration: (...args: unknown[]) =>
			mockState.authService.startPasskeyRegistration(...args),
		uploadAvatar: (...args: unknown[]) =>
			mockState.authService.uploadAvatar(...args),
	},
}));

vi.mock("@/lib/webauthn", () => ({
	createPasskeyCredential: (...args: unknown[]) =>
		mockState.webAuthn.createPasskeyCredential(...args),
	isWebAuthnSupported: () => mockState.webAuthn.supported,
	WebAuthnCancelledError: class WebAuthnCancelledError extends Error {},
	WebAuthnUnsupportedError: class WebAuthnUnsupportedError extends Error {},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (selector: (state: typeof mockState.authStore) => unknown) =>
		selector(mockState.authStore),
	forceLogout: () => mockState.authStore.forceLogout(),
}));

vi.mock("@/stores/displayTimeZoneStore", () => ({
	ALL_DISPLAY_TIME_ZONES: ["America/Los_Angeles"],
	COMMON_DISPLAY_TIME_ZONES: ["UTC", "Asia/Shanghai"],
	DISPLAY_TIME_ZONE_BROWSER: "browser",
	getActiveDisplayTimeZone: () =>
		mockState.displayTimeZoneStore.preference === "browser"
			? "UTC"
			: mockState.displayTimeZoneStore.preference,
	resolveBrowserTimeZone: () => "UTC",
	useDisplayTimeZoneStore: (
		selector: (state: typeof mockState.displayTimeZoneStore) => unknown,
	) => selector(mockState.displayTimeZoneStore),
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: (selector: (state: typeof mockState.fileStore) => unknown) =>
		selector(mockState.fileStore),
}));

vi.mock("@/stores/themeStore", () => ({
	useThemeStore: () => mockState.themeStore,
}));

describe("SettingsPage", () => {
	const currentSessionUserAgent =
		"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36 Edg/147.0.0.0";
	const otherSessionUserAgent =
		"Mozilla/5.0 (iPhone; CPU iPhone OS 18_3 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Mobile/15E148 Safari/604.1";

	beforeEach(() => {
		mockState.authService.changePassword.mockReset();
		mockState.authService.changePassword.mockResolvedValue({ expiresIn: 900 });
		mockState.authService.deletePasskey.mockReset();
		mockState.authService.deletePasskey.mockResolvedValue(undefined);
		mockState.authService.finishPasskeyRegistration.mockReset();
		mockState.authService.finishPasskeyRegistration.mockResolvedValue({
			backed_up: true,
			backup_eligible: true,
			created_at: "2026-05-01T08:00:00Z",
			id: 2,
			last_used_at: null,
			name: "Laptop",
			sign_count: 0,
			transports: null,
			updated_at: "2026-05-01T08:00:00Z",
		});
		mockState.authService.listPasskeys.mockReset();
		mockState.authService.listPasskeys.mockResolvedValue([]);
		mockState.authService.listSessions.mockReset();
		mockState.authService.listSessions.mockResolvedValue([
			{
				created_at: "2026-04-20T08:00:00Z",
				expires_at: "2026-04-27T08:00:00Z",
				id: "session-current",
				ip_address: "127.0.0.1",
				is_current: true,
				last_seen_at: "2026-04-20T09:00:00Z",
				user_agent: currentSessionUserAgent,
			},
			{
				created_at: "2026-04-19T08:00:00Z",
				expires_at: "2026-04-26T08:00:00Z",
				id: "session-other",
				ip_address: "192.168.1.10",
				is_current: false,
				last_seen_at: "2026-04-20T07:00:00Z",
				user_agent: otherSessionUserAgent,
			},
		]);
		mockState.authService.revokeOtherSessions.mockReset();
		mockState.authService.revokeOtherSessions.mockResolvedValue(1);
		mockState.authService.revokeSession.mockReset();
		mockState.authService.revokeSession.mockResolvedValue(undefined);
		mockState.authService.renamePasskey.mockReset();
		mockState.authService.renamePasskey.mockImplementation(
			(id: number, payload: { name: string }) => ({
				backed_up: false,
				backup_eligible: true,
				created_at: "2026-04-01T08:00:00Z",
				id,
				last_used_at: null,
				name: payload.name,
				sign_count: 0,
				transports: null,
				updated_at: "2026-05-01T08:00:00Z",
			}),
		);
		mockState.authService.requestEmailChange.mockReset();
		mockState.authService.resendEmailChange.mockReset();
		mockState.authService.setAvatarSource.mockReset();
		mockState.authService.startPasskeyRegistration.mockReset();
		mockState.authService.startPasskeyRegistration.mockResolvedValue({
			flow_id: "passkey-flow",
			public_key: { publicKey: { challenge: "AQID" } },
		});
		mockState.authService.uploadAvatar.mockReset();
		mockState.authService.updateProfile.mockReset();
		mockState.authStore.forceLogout.mockReset();
		mockState.authStore.refreshUser.mockReset();
		mockState.authStore.setStorageEventStreamEnabled.mockReset();
		mockState.authStore.syncSession.mockReset();
		mockState.authStore.user.preferences.storage_event_stream_enabled = true;
		mockState.changeLanguage.mockReset();
		mockState.displayTimeZoneStore.preference = "browser";
		mockState.displayTimeZoneStore.setPreference.mockReset();
		mockState.fileStore.browserOpenMode = "double_click";
		mockState.fileStore.setBrowserOpenMode.mockReset();
		mockState.fileStore.setViewMode.mockReset();
		mockState.fileStore.viewMode = "list";
		mockState.navigate.mockReset();
		mockState.preferenceSync.mockReset();
		mockState.themeStore.mode = "dark";
		mockState.themeStore.setMode.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.translationLanguage = "zh-CN";
		mockState.webAuthn.createPasskeyCredential.mockReset();
		mockState.webAuthn.createPasskeyCredential.mockResolvedValue({
			id: "credential-1",
		});
		mockState.webAuthn.supported = false;
		mockState.location = {
			hash: "",
			pathname: "/settings/security",
			search: "",
			state: null,
		};
	});

	it("renders current descriptions from the selected theme, language, browser mode, and open mode", () => {
		render(<SettingsPage section="interface" />);

		expect(screen.getByTestId("app-layout")).toBeInTheDocument();
		expect(screen.getByText("settings")).toBeInTheDocument();
		expect(screen.getByText("settings:settings_page_desc")).toBeInTheDocument();
		expect(
			screen.getByText("settings:settings_theme_dark_desc"),
		).toBeInTheDocument();
		expect(
			screen.getByText("settings:settings_language_zh_desc"),
		).toBeInTheDocument();
		expect(
			screen.getByText(
				'settings:settings_display_time_zone_browser_desc:{"timezone":"UTC"}',
			),
		).toBeInTheDocument();
		expect(
			screen.getByText("settings:settings_browser_list_desc"),
		).toBeInTheDocument();
		expect(
			screen.getByText("settings:settings_browser_open_double_click_desc"),
		).toBeInTheDocument();
		expect(screen.getByText("color-preset-picker")).toBeInTheDocument();
		expect(screen.getAllByTestId("choice-group")[0]).toHaveAttribute(
			"data-value",
			"dark",
		);
		expect(screen.getAllByTestId("choice-group")[1]).toHaveAttribute(
			"data-value",
			"zh",
		);
		expect(screen.getAllByTestId("choice-group")[2]).toHaveAttribute(
			"data-value",
			"list",
		);
		expect(screen.getAllByTestId("choice-group")[3]).toHaveAttribute(
			"data-value",
			"double_click",
		);
		expect(
			screen.getByRole("switch", {
				name: "settings:settings_storage_event_stream",
			}),
		).toHaveAttribute("data-checked");
		expect(
			screen.getByText("settings:settings_storage_event_stream_enabled_desc"),
		).toBeInTheDocument();
	});

	it("dispatches theme, language, time zone, browser, open mode, and realtime sync preference changes", () => {
		render(<SettingsPage section="interface" />);

		fireEvent.click(screen.getByRole("button", { name: "theme_light" }));
		fireEvent.click(screen.getByRole("button", { name: "language_en" }));
		fireEvent.click(
			screen.getByRole("button", { name: "America/Los_Angeles" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "files:grid_view" }));
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_browser_open_single_click",
			}),
		);
		fireEvent.click(
			screen.getByRole("switch", {
				name: "settings:settings_storage_event_stream",
			}),
		);

		expect(mockState.themeStore.setMode).toHaveBeenCalledWith("light");
		expect(mockState.changeLanguage).toHaveBeenCalledWith("en");
		expect(mockState.preferenceSync).toHaveBeenCalledWith({ language: "en" });
		expect(mockState.displayTimeZoneStore.setPreference).toHaveBeenCalledWith(
			"America/Los_Angeles",
		);
		expect(mockState.fileStore.setViewMode).toHaveBeenCalledWith("grid");
		expect(mockState.fileStore.setBrowserOpenMode).toHaveBeenCalledWith(
			"single_click",
		);
		expect(
			mockState.authStore.setStorageEventStreamEnabled,
		).toHaveBeenCalledWith(false);
		expect(mockState.preferenceSync).toHaveBeenCalledWith({
			storage_event_stream_enabled: false,
		});
	});

	it("navigates between split settings sections from the top tabs", () => {
		render(<SettingsPage section="profile" />);

		fireEvent.click(
			screen.getByRole("button", { name: "settings:settings_interface" }),
		);

		expect(mockState.navigate).toHaveBeenCalledWith("/settings/interface", {
			viewTransition: false,
		});
	});

	it("only animates the settings panel after section changes, not on initial entry", () => {
		const { rerender } = render(<SettingsPage section="profile" />);

		expect(screen.getByTestId("settings-panel")).not.toHaveClass("animate-in");

		rerender(<SettingsPage section="interface" />);

		expect(screen.getByTestId("settings-panel")).toHaveClass("animate-in");
		expect(screen.getByTestId("settings-panel")).toHaveClass(
			"slide-in-from-right-4",
		);
	});

	it("loads and revokes other auth sessions from security settings", async () => {
		render(<SettingsPage section="security" />);

		await waitFor(() =>
			expect(mockState.authService.listSessions).toHaveBeenCalled(),
		);
		expect(
			screen.getByText("settings:settings_sessions_section"),
		).toBeInTheDocument();
		expect(
			screen.getByText(
				"Edge 147 · macOS 10.15.7 · settings:settings_sessions_device_desktop",
			),
		).toHaveAttribute("title", currentSessionUserAgent);
		expect(
			screen.getByText(
				"Safari 18.3 · iOS 18.3 · settings:settings_sessions_device_mobile",
			),
		).toHaveAttribute("title", otherSessionUserAgent);

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_sessions_revoke_others",
			}),
		);

		await waitFor(() =>
			expect(mockState.authService.revokeOtherSessions).toHaveBeenCalledTimes(
				1,
			),
		);
		expect(mockState.toastSuccess).toHaveBeenCalled();
	});

	it("lists passkeys and adds a supported passkey from security settings", async () => {
		mockState.webAuthn.supported = true;
		mockState.authService.listPasskeys.mockResolvedValueOnce([
			{
				backed_up: false,
				backup_eligible: true,
				created_at: "2026-04-01T08:00:00Z",
				id: 1,
				last_used_at: null,
				name: "Phone",
				sign_count: 0,
				transports: null,
				updated_at: "2026-04-01T08:00:00Z",
			},
		]);

		render(<SettingsPage section="security" />);

		await waitFor(() =>
			expect(mockState.authService.listPasskeys).toHaveBeenCalledTimes(1),
		);
		expect(screen.getByText("Phone")).toBeInTheDocument();

		fireEvent.change(
			screen.getByLabelText("settings:settings_passkeys_new_name"),
			{
				target: { value: "Laptop" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_passkeys_add",
			}),
		);

		await waitFor(() => {
			expect(
				mockState.authService.startPasskeyRegistration,
			).toHaveBeenCalledWith({
				name: "Laptop",
			});
			expect(mockState.webAuthn.createPasskeyCredential).toHaveBeenCalledWith({
				publicKey: { challenge: "AQID" },
			});
			expect(
				mockState.authService.finishPasskeyRegistration,
			).toHaveBeenCalledWith("passkey-flow", { id: "credential-1" }, "Laptop");
		});
		expect(
			await screen.findByText("settings:settings_passkeys_synced"),
		).toBeInTheDocument();
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_passkeys_added",
		);
	});

	it("saves the password through the security endpoint", async () => {
		render(<SettingsPage section="security" />);

		fireEvent.change(
			screen.getByLabelText("settings:settings_password_current"),
			{
				target: { value: "pass123" },
			},
		);
		fireEvent.change(screen.getByLabelText("settings:settings_password_new"), {
			target: { value: "newsecret456" },
		});
		fireEvent.change(
			screen.getByLabelText("settings:settings_password_confirm"),
			{
				target: { value: "newsecret456" },
			},
		);
		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() =>
			expect(mockState.authService.changePassword).toHaveBeenCalledWith({
				current_password: "pass123",
				new_password: "newsecret456",
			}),
		);
		expect(mockState.authStore.syncSession).toHaveBeenCalledWith(900);
	});

	it("shows a query toast after redirecting into security settings", async () => {
		mockState.location = {
			hash: "",
			pathname: "/settings/security",
			search: "?contact_verification=email-changed&email=updated%40example.com",
		};

		render(<SettingsPage section="security" />);

		await waitFor(() =>
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				'settings:settings_email_change_confirmed:{"email":"updated@example.com"}',
				{
					id: "contact-verification-email-changed-settings:updated@example.com",
				},
			),
		);
		expect(mockState.navigate).toHaveBeenCalledWith(
			{
				hash: "",
				pathname: "/settings/security",
				search: "",
			},
			{ replace: true },
		);
	});

	it("saves the display name through the profile endpoint", async () => {
		render(<SettingsPage section="profile" />);

		expect(screen.getByText("avatar:alice")).toBeInTheDocument();
		expect(screen.getByDisplayValue("alice@example.com")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("settings:settings_display_name"), {
			target: { value: "Alice Chen" },
		});
		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() =>
			expect(mockState.authService.updateProfile).toHaveBeenCalledWith({
				display_name: "Alice Chen",
			}),
		);
		await waitFor(() =>
			expect(mockState.authStore.refreshUser).toHaveBeenCalledTimes(1),
		);
	});

	it("uploads the confirmed cropped avatar through the profile flow", async () => {
		const { container } = render(<SettingsPage section="profile" />);
		const fileInput = container.querySelector('input[type="file"]');
		const originalFile = new File(["raw"], "portrait.png", {
			type: "image/png",
		});

		expect(fileInput).not.toBeNull();

		fireEvent.change(fileInput as HTMLInputElement, {
			target: { files: [originalFile] },
		});

		expect(screen.getByTestId("avatar-crop-dialog")).toBeInTheDocument();
		expect(screen.getByText("portrait.png")).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_avatar_crop_apply",
			}),
		);

		await waitFor(() =>
			expect(mockState.authService.uploadAvatar).toHaveBeenCalledTimes(1),
		);
		await waitFor(() =>
			expect(mockState.authStore.refreshUser).toHaveBeenCalledTimes(1),
		);

		const uploadedFile = mockState.authService.uploadAvatar.mock.calls[0]?.[0];
		expect(uploadedFile).toBeInstanceOf(File);
		expect(uploadedFile?.name).toBe("cropped-avatar.webp");
	});
});

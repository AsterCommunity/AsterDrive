import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SecurityMfaSection } from "@/components/settings/security-settings/SecurityMfaSection";
import type { MfaFactorInfo, MfaStatus } from "@/services/authService";

const mockState = vi.hoisted(() => ({
	authService: {
		deleteMfaFactor: vi.fn(),
		finishTotpSetup: vi.fn(),
		getMfaStatus: vi.fn(),
		regenerateMfaRecoveryCodes: vi.fn(),
		startTotpSetup: vi.fn(),
	},
	clipboard: vi.fn(),
	downloadedLinks: [] as HTMLAnchorElement[],
	handleApiError: vi.fn(),
	toastInfo: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, vars?: Record<string, unknown>) =>
			vars ? `${key}:${JSON.stringify(vars)}` : key,
	}),
}));

vi.mock("qrcode", () => ({
	default: {
		create: () => ({
			modules: {
				size: 2,
				get: (row: number, col: number) => row === col,
			},
		}),
	},
}));

vi.mock("sonner", () => ({
	toast: {
		info: (...args: unknown[]) => mockState.toastInfo(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		variant,
	}: {
		children: React.ReactNode;
		variant?: string;
	}) => <span data-variant={variant}>{children}</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		className,
		disabled,
		onClick,
		type,
		variant,
		...props
	}: {
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
		variant?: string;
	}) => (
		<button
			{...props}
			type={type ?? "button"}
			className={className}
			data-variant={variant}
			disabled={disabled}
			onClick={onClick}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({ ...props }: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
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

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: (...args: unknown[]) => mockState.clipboard(...args),
}));

vi.mock("@/lib/format", () => ({
	formatDateAbsolute: (value: string) => `date:${value}`,
}));

vi.mock("@/services/authService", () => ({
	authService: {
		deleteMfaFactor: (...args: unknown[]) =>
			mockState.authService.deleteMfaFactor(...args),
		finishTotpSetup: (...args: unknown[]) =>
			mockState.authService.finishTotpSetup(...args),
		getMfaStatus: (...args: unknown[]) =>
			mockState.authService.getMfaStatus(...args),
		regenerateMfaRecoveryCodes: (...args: unknown[]) =>
			mockState.authService.regenerateMfaRecoveryCodes(...args),
		startTotpSetup: (...args: unknown[]) =>
			mockState.authService.startTotpSetup(...args),
	},
}));

function factor(overrides: Partial<MfaFactorInfo> = {}): MfaFactorInfo {
	return {
		enabled_at: "2026-05-01T08:00:00Z",
		id: 7,
		last_used_at: null,
		method: "totp",
		name: "Authenticator app",
		...overrides,
	};
}

function status(overrides: Partial<MfaStatus> = {}): MfaStatus {
	return {
		enabled: false,
		factors: [],
		recovery_codes_remaining: 0,
		...overrides,
	};
}

describe("SecurityMfaSection", () => {
	beforeEach(() => {
		mockState.authService.deleteMfaFactor.mockReset();
		mockState.authService.deleteMfaFactor.mockResolvedValue(undefined);
		mockState.authService.finishTotpSetup.mockReset();
		mockState.authService.finishTotpSetup.mockResolvedValue({
			factor: factor(),
			recovery_codes: ["AAAA-BBBB-CCCC", "DDDD-EEEE-FFFF"],
		});
		mockState.authService.getMfaStatus.mockReset();
		mockState.authService.getMfaStatus.mockResolvedValue(status());
		mockState.authService.regenerateMfaRecoveryCodes.mockReset();
		mockState.authService.regenerateMfaRecoveryCodes.mockResolvedValue([
			"GGGG-HHHH-IIII",
			"JJJJ-KKKK-LLLL",
		]);
		mockState.authService.startTotpSetup.mockReset();
		mockState.authService.startTotpSetup.mockResolvedValue({
			expires_in: 300,
			flow_token: "setup-flow",
			otpauth_uri: "otpauth://totp/AsterDrive:alice?secret=SECRET123",
			secret: "SECRET123",
		});
		mockState.clipboard.mockReset();
		mockState.clipboard.mockResolvedValue(undefined);
		mockState.downloadedLinks = [];
		mockState.handleApiError.mockReset();
		mockState.toastInfo.mockReset();
		mockState.toastSuccess.mockReset();
		if (!URL.createObjectURL) {
			Object.defineProperty(URL, "createObjectURL", {
				configurable: true,
				value: vi.fn(),
			});
		}
		if (!URL.revokeObjectURL) {
			Object.defineProperty(URL, "revokeObjectURL", {
				configurable: true,
				value: vi.fn(),
			});
		}
		vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(
			function click(this: HTMLAnchorElement) {
				mockState.downloadedLinks.push(this);
			},
		);
		vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:recovery-codes");
		vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
	});

	it("loads an empty MFA status, refreshes, and reports load errors", async () => {
		render(<SecurityMfaSection />);

		expect(
			await screen.findByText("settings:settings_mfa_empty"),
		).toBeInTheDocument();
		expect(
			screen.getByText("settings:settings_mfa_disabled_badge"),
		).toBeInTheDocument();

		const error = new Error("mfa status failed");
		mockState.authService.getMfaStatus.mockRejectedValueOnce(error);
		fireEvent.click(screen.getByRole("button", { name: "core:refresh" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(mockState.authService.getMfaStatus).toHaveBeenCalledTimes(2);
	});

	it("walks through TOTP setup, stores recovery codes, and closes after confirmation", async () => {
		render(<SecurityMfaSection />);

		fireEvent.click(
			await screen.findByRole("button", {
				name: "settings:settings_mfa_start_setup",
			}),
		);
		expect(
			screen.getByText("settings:settings_mfa_intro_title"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_intro_continue",
			}),
		);

		await waitFor(() => {
			expect(mockState.authService.startTotpSetup).toHaveBeenCalledTimes(1);
		});
		expect(
			await screen.findByRole("img", {
				name: "settings:settings_mfa_qr_alt",
			}),
		).toBeInTheDocument();
		expect(screen.getByDisplayValue("••••••••••••••••")).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_show_secret",
			}),
		);
		expect(screen.getByDisplayValue("SECRET123")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "core:copy" }));
		await waitFor(() => {
			expect(mockState.clipboard).toHaveBeenCalledWith("SECRET123");
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_mfa_secret_copied",
		);

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_scan_continue",
			}),
		);
		fireEvent.change(
			await screen.findByLabelText("settings:settings_mfa_factor_name"),
			{
				target: { value: "  Work phone  " },
			},
		);
		fireEvent.change(screen.getByLabelText("settings:settings_mfa_totp_code"), {
			target: { value: "12a34 56" },
		});
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_finish_setup",
			}),
		);

		await waitFor(() => {
			expect(mockState.authService.finishTotpSetup).toHaveBeenCalledWith({
				code: "123456",
				flow_token: "setup-flow",
				name: "Work phone",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_mfa_enabled",
		);

		expect(await screen.findByText("AAAA-BBBB-CCCC")).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_copy_recovery_codes",
			}),
		);
		await waitFor(() => {
			expect(mockState.clipboard).toHaveBeenLastCalledWith(
				expect.stringContaining("AAAA-BBBB-CCCC"),
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_mfa_recovery_copied",
		);
		expect(
			screen.getByRole("button", { name: "settings:settings_mfa_done" }),
		).toBeEnabled();

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_download_recovery_codes",
			}),
		);
		expect(mockState.downloadedLinks[0]?.download).toBe(
			"asterdrive-mfa-recovery-codes.txt",
		);

		fireEvent.click(
			screen.getByRole("button", { name: "settings:settings_mfa_done" }),
		);
		const exitingRecoveryPanel = screen
			.getByText("settings:settings_mfa_recovery_codes_title")
			.closest('[aria-hidden="true"]');
		expect(exitingRecoveryPanel).not.toBeNull();

		fireEvent.transitionEnd(exitingRecoveryPanel as Element);
		expect(
			screen.queryByText("settings:settings_mfa_recovery_codes_title"),
		).not.toBeInTheDocument();
	});

	it("opens the disable MFA code entry directly and disables the factor", async () => {
		mockState.authService.getMfaStatus.mockResolvedValue(
			status({
				enabled: true,
				factors: [factor()],
				recovery_codes_remaining: 6,
			}),
		);

		render(<SecurityMfaSection />);

		expect(await screen.findByText("Authenticator app")).toBeInTheDocument();
		expect(screen.getByText(/TOTP ·/)).toBeInTheDocument();
		expect(
			screen.getByText('settings:settings_mfa_recovery_remaining:{"count":6}'),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_disable",
			}),
		);
		expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
		expect(
			screen.getByText(/settings:settings_mfa_disable_desc/),
		).toHaveTextContent("settings:settings_mfa_sensitive_action_desc");
		expect(
			screen.getByLabelText("settings:settings_mfa_code_or_recovery"),
		).toBeInTheDocument();
		fireEvent.change(
			screen.getByLabelText("settings:settings_mfa_code_or_recovery"),
			{
				target: { value: "123456" },
			},
		);

		fireEvent.change(
			screen.getByLabelText("settings:settings_mfa_code_or_recovery"),
			{
				target: { value: "123456" },
			},
		);
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "settings:settings_mfa_disable",
			})[1],
		);

		await waitFor(() => {
			expect(mockState.authService.deleteMfaFactor).toHaveBeenCalledWith(7, {
				code: "123456",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_mfa_disabled",
		);
	});

	it("reports a stale disable action when no factor exists", async () => {
		mockState.authService.getMfaStatus.mockResolvedValue(
			status({ enabled: true, factors: [factor()] }),
		);

		render(<SecurityMfaSection />);

		expect(await screen.findByText("Authenticator app")).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_disable",
			}),
		);

		mockState.authService.getMfaStatus.mockResolvedValue(status());
		fireEvent.click(screen.getByRole("button", { name: "core:refresh" }));
		await screen.findByText("settings:settings_mfa_empty");
		fireEvent.change(
			screen.getByLabelText("settings:settings_mfa_code_or_recovery"),
			{
				target: { value: "123456" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_disable",
			}),
		);

		await waitFor(() => {
			expect(mockState.toastInfo).toHaveBeenCalledWith(
				"settings:settings_mfa_disable_missing_factor",
			);
		});
		expect(mockState.authService.deleteMfaFactor).not.toHaveBeenCalled();
	});

	it("regenerates recovery codes and keeps failed sensitive actions editable", async () => {
		const error = new Error("bad code");
		mockState.authService.getMfaStatus.mockResolvedValue(
			status({ enabled: true, factors: [factor()] }),
		);
		mockState.authService.regenerateMfaRecoveryCodes.mockRejectedValueOnce(
			error,
		);

		render(<SecurityMfaSection />);

		await screen.findByText("Authenticator app");
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_regenerate_recovery",
			}),
		);
		expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
		expect(
			screen.getByText(/settings:settings_mfa_regenerate_desc/),
		).toHaveTextContent("settings:settings_mfa_sensitive_action_desc");
		fireEvent.change(
			screen.getByLabelText("settings:settings_mfa_code_or_recovery"),
			{
				target: { value: "bad-code" },
			},
		);
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "settings:settings_mfa_regenerate_recovery",
			})[1],
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(
			screen.getByLabelText("settings:settings_mfa_code_or_recovery"),
		).toHaveValue("bad-code");

		mockState.handleApiError.mockReset();
		fireEvent.change(
			screen.getByLabelText("settings:settings_mfa_code_or_recovery"),
			{
				target: { value: "654321" },
			},
		);
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "settings:settings_mfa_regenerate_recovery",
			})[1],
		);

		await waitFor(() => {
			expect(
				mockState.authService.regenerateMfaRecoveryCodes,
			).toHaveBeenLastCalledWith({ code: "654321" });
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_mfa_recovery_regenerated",
		);
		expect(await screen.findByText("GGGG-HHHH-IIII")).toBeInTheDocument();
	});

	it("reports setup and clipboard failures through the shared API error handler", async () => {
		const startError = new Error("setup failed");
		mockState.authService.startTotpSetup.mockRejectedValueOnce(startError);

		render(<SecurityMfaSection />);

		fireEvent.click(
			await screen.findByRole("button", {
				name: "settings:settings_mfa_start_setup",
			}),
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_intro_continue",
			}),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(startError);
		});
		expect(
			screen.getByText("settings:settings_mfa_intro_title"),
		).toBeInTheDocument();

		mockState.handleApiError.mockReset();
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_mfa_intro_continue",
			}),
		);
		await screen.findByRole("img", {
			name: "settings:settings_mfa_qr_alt",
		});

		const copyError = new Error("clipboard blocked");
		mockState.clipboard.mockRejectedValueOnce(copyError);
		fireEvent.click(screen.getByRole("button", { name: "core:copy" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(copyError);
		});
	});
});

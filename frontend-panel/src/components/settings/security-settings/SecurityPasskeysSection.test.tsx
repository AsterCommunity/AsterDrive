import {
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SecurityPasskeysSection } from "@/components/settings/security-settings/SecurityPasskeysSection";
import type { PasskeyInfo } from "@/types/api";

const MockWebAuthnCancelledError = vi.hoisted(
	() =>
		class MockWebAuthnCancelledError extends Error {
			constructor(message = "cancelled") {
				super(message);
				this.name = "WebAuthnCancelledError";
			}
		},
);

const MockWebAuthnUnsupportedError = vi.hoisted(
	() =>
		class MockWebAuthnUnsupportedError extends Error {
			constructor(message = "unsupported") {
				super(message);
				this.name = "WebAuthnUnsupportedError";
			}
		},
);

const mockState = vi.hoisted(() => ({
	authService: {
		deletePasskey: vi.fn(),
		finishPasskeyRegistration: vi.fn(),
		listPasskeys: vi.fn(),
		renamePasskey: vi.fn(),
		startPasskeyRegistration: vi.fn(),
	},
	createPasskeyCredential: vi.fn(),
	handleApiError: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	webAuthnSupported: true,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		confirmLabel,
		description,
		onConfirm,
		open,
		title,
	}: {
		confirmLabel: string;
		description?: string;
		onConfirm: () => void;
		open: boolean;
		title: string;
	}) =>
		open ? (
			<div role="dialog">
				<h2>{title}</h2>
				<p>{description}</p>
				<button type="button" onClick={onConfirm}>
					{confirmLabel}
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button type={type ?? "button"} disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
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

vi.mock("@/lib/format", () => ({
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatDateAbsoluteWithOffset: (value: string) => `offset:${value}`,
}));

vi.mock("@/lib/webauthn", () => ({
	createPasskeyCredential: (...args: unknown[]) =>
		mockState.createPasskeyCredential(...args),
	isWebAuthnSupported: () => mockState.webAuthnSupported,
	WebAuthnCancelledError: MockWebAuthnCancelledError,
	WebAuthnUnsupportedError: MockWebAuthnUnsupportedError,
}));

vi.mock("@/services/authService", () => ({
	authService: {
		deletePasskey: (...args: unknown[]) =>
			mockState.authService.deletePasskey(...args),
		finishPasskeyRegistration: (...args: unknown[]) =>
			mockState.authService.finishPasskeyRegistration(...args),
		listPasskeys: (...args: unknown[]) =>
			mockState.authService.listPasskeys(...args),
		renamePasskey: (...args: unknown[]) =>
			mockState.authService.renamePasskey(...args),
		startPasskeyRegistration: (...args: unknown[]) =>
			mockState.authService.startPasskeyRegistration(...args),
	},
}));

function passkey(overrides: Partial<PasskeyInfo> = {}): PasskeyInfo {
	return {
		backed_up: false,
		backup_eligible: true,
		created_at: "2026-04-01T08:00:00Z",
		id: 1,
		last_used_at: null,
		name: "Phone",
		sign_count: 0,
		transports: null,
		updated_at: "2026-04-01T08:00:00Z",
		...overrides,
	};
}

describe("SecurityPasskeysSection", () => {
	beforeEach(() => {
		mockState.authService.deletePasskey.mockReset();
		mockState.authService.deletePasskey.mockResolvedValue(undefined);
		mockState.authService.finishPasskeyRegistration.mockReset();
		mockState.authService.finishPasskeyRegistration.mockResolvedValue(
			passkey({
				backed_up: true,
				created_at: "2026-05-01T08:00:00Z",
				id: 2,
				name: "Laptop",
			}),
		);
		mockState.authService.listPasskeys.mockReset();
		mockState.authService.listPasskeys.mockResolvedValue([]);
		mockState.authService.renamePasskey.mockReset();
		mockState.authService.renamePasskey.mockImplementation(
			(id: number, payload: { name: string }) =>
				passkey({ id, name: payload.name }),
		);
		mockState.authService.startPasskeyRegistration.mockReset();
		mockState.authService.startPasskeyRegistration.mockResolvedValue({
			flow_id: "register-flow",
			public_key: { publicKey: { challenge: "AQID" } },
		});
		mockState.createPasskeyCredential.mockReset();
		mockState.createPasskeyCredential.mockResolvedValue({ id: "credential-1" });
		mockState.handleApiError.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.webAuthnSupported = true;
	});

	it("loads an empty supported passkey list and refreshes errors through the shared handler", async () => {
		render(<SecurityPasskeysSection />);

		await waitFor(() =>
			expect(mockState.authService.listPasskeys).toHaveBeenCalledTimes(1),
		);
		expect(
			screen.getByText("settings:settings_passkeys_empty"),
		).toBeInTheDocument();
		expect(
			screen.getByText("settings:settings_passkeys_add_hint"),
		).toBeInTheDocument();

		const error = new Error("refresh failed");
		mockState.authService.listPasskeys.mockRejectedValueOnce(error);
		fireEvent.click(screen.getByRole("button", { name: /core:refresh/ }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
	});

	it("shows the unsupported state when WebAuthn is unavailable", async () => {
		mockState.webAuthnSupported = false;

		render(<SecurityPasskeysSection />);

		expect(
			await screen.findByText("auth:passkey_unsupported"),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /settings:settings_passkeys_add/ }),
		).toBeDisabled();
	});

	it("adds a passkey using the default name when the input is blank", async () => {
		render(<SecurityPasskeysSection />);

		fireEvent.click(
			await screen.findByRole("button", {
				name: /settings:settings_passkeys_add/,
			}),
		);

		await waitFor(() =>
			expect(
				mockState.authService.startPasskeyRegistration,
			).toHaveBeenCalledWith({
				name: "settings:settings_passkeys_default_name",
			}),
		);
		expect(mockState.createPasskeyCredential).toHaveBeenCalledWith({
			publicKey: { challenge: "AQID" },
		});
		expect(
			mockState.authService.finishPasskeyRegistration,
		).toHaveBeenCalledWith(
			"register-flow",
			{ id: "credential-1" },
			"settings:settings_passkeys_default_name",
		);
		expect(await screen.findByText("Laptop")).toBeInTheDocument();
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_passkeys_added",
		);
	});

	it("shows unsupported and cancelled WebAuthn errors without calling the API error handler", async () => {
		mockState.createPasskeyCredential.mockRejectedValueOnce(
			new MockWebAuthnUnsupportedError(),
		);

		render(<SecurityPasskeysSection />);

		fireEvent.change(
			await screen.findByLabelText("settings:settings_passkeys_new_name"),
			{
				target: { value: "Laptop" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", { name: /settings:settings_passkeys_add/ }),
		);

		await waitFor(() =>
			expect(mockState.toastError).toHaveBeenCalledWith(
				"auth:passkey_unsupported",
			),
		);

		mockState.createPasskeyCredential.mockRejectedValueOnce(
			new MockWebAuthnCancelledError(),
		);
		fireEvent.click(
			screen.getByRole("button", { name: /settings:settings_passkeys_add/ }),
		);

		await waitFor(() =>
			expect(mockState.toastError).toHaveBeenCalledWith(
				"auth:passkey_cancelled",
			),
		);
		expect(mockState.handleApiError).not.toHaveBeenCalled();
	});

	it("renames, cancels editing, and deletes existing passkeys", async () => {
		mockState.authService.listPasskeys.mockResolvedValueOnce([
			passkey({
				backed_up: true,
				last_used_at: "2026-04-03T08:00:00Z",
			}),
		]);

		render(<SecurityPasskeysSection />);

		expect(await screen.findByText("Phone")).toBeInTheDocument();
		expect(
			screen.getByText("settings:settings_passkeys_synced"),
		).toBeInTheDocument();
		expect(screen.getByText("date:2026-04-03T08:00:00Z")).toHaveAttribute(
			"title",
			"offset:2026-04-03T08:00:00Z",
		);

		fireEvent.click(
			screen.getByRole("button", {
				name: /settings:settings_passkeys_rename/,
			}),
		);
		fireEvent.change(
			screen.getByLabelText("settings:settings_passkeys_edit_name"),
			{
				target: { value: "Tablet" },
			},
		);
		fireEvent.click(screen.getByRole("button", { name: /core:save/ }));

		await waitFor(() =>
			expect(mockState.authService.renamePasskey).toHaveBeenCalledWith(1, {
				name: "Tablet",
			}),
		);
		expect(await screen.findByText("Tablet")).toBeInTheDocument();
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_passkeys_renamed",
		);

		fireEvent.click(
			screen.getByRole("button", {
				name: /settings:settings_passkeys_rename/,
			}),
		);
		fireEvent.change(
			screen.getByLabelText("settings:settings_passkeys_edit_name"),
			{
				target: { value: "   " },
			},
		);
		expect(screen.getByRole("button", { name: /core:save/ })).toBeDisabled();
		fireEvent.click(screen.getByRole("button", { name: /core:cancel/ }));
		expect(
			screen.queryByLabelText("settings:settings_passkeys_edit_name"),
		).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: /settings:settings_passkeys_delete/,
			}),
		);
		expect(
			screen.getByText("settings:settings_passkeys_delete_title"),
		).toBeInTheDocument();
		fireEvent.click(
			within(screen.getByRole("dialog")).getByRole("button", {
				name: "settings:settings_passkeys_delete",
			}),
		);

		await waitFor(() =>
			expect(mockState.authService.deletePasskey).toHaveBeenCalledWith(1),
		);
		expect(screen.queryByText("Tablet")).not.toBeInTheDocument();
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_passkeys_deleted",
		);
	});

	it("reports API failures from create, rename, and delete flows", async () => {
		const createError = new Error("create failed");
		const renameError = new Error("rename failed");
		const deleteError = new Error("delete failed");
		mockState.authService.finishPasskeyRegistration.mockRejectedValueOnce(
			createError,
		);
		mockState.authService.listPasskeys.mockResolvedValueOnce([passkey()]);
		mockState.authService.renamePasskey.mockRejectedValueOnce(renameError);
		mockState.authService.deletePasskey.mockRejectedValueOnce(deleteError);

		render(<SecurityPasskeysSection />);

		fireEvent.click(
			await screen.findByRole("button", {
				name: /settings:settings_passkeys_add/,
			}),
		);
		await waitFor(() =>
			expect(mockState.handleApiError).toHaveBeenCalledWith(createError),
		);

		fireEvent.click(
			screen.getByRole("button", {
				name: /settings:settings_passkeys_rename/,
			}),
		);
		fireEvent.change(
			screen.getByLabelText("settings:settings_passkeys_edit_name"),
			{
				target: { value: "Tablet" },
			},
		);
		fireEvent.click(screen.getByRole("button", { name: /core:save/ }));
		await waitFor(() =>
			expect(mockState.handleApiError).toHaveBeenCalledWith(renameError),
		);

		fireEvent.click(
			screen.getByRole("button", {
				name: /settings:settings_passkeys_delete/,
			}),
		);
		fireEvent.click(
			within(screen.getByRole("dialog")).getByRole("button", {
				name: "settings:settings_passkeys_delete",
			}),
		);
		await waitFor(() =>
			expect(mockState.handleApiError).toHaveBeenCalledWith(deleteError),
		);
	});
});

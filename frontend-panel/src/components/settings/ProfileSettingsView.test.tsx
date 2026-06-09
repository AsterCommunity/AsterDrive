import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileSettingsView } from "@/components/settings/ProfileSettingsView";
import type { MeResponse } from "@/types/api";

const mockState = vi.hoisted(() => ({
	authService: {
		setAvatarSource: vi.fn(),
		updateProfile: vi.fn(),
		uploadAvatar: vi.fn(),
	},
	handleApiError: vi.fn(),
	refreshUser: vi.fn(),
	toastSuccess: vi.fn(),
	user: null as MeResponse | null,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options ? `${key}:${JSON.stringify(options)}` : key,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/UserAvatarImage", () => ({
	UserAvatarImage: ({
		avatar,
		name,
	}: {
		avatar: { source: string } | null;
		name: string;
	}) => (
		<div data-testid="avatar" data-source={avatar?.source ?? "none"}>
			{name}
		</div>
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
		open && file ? (
			<dialog open aria-label="avatar-crop">
				<span>{file.name}</span>
				<button type="button" onClick={() => void onConfirm(file)}>
					confirm-avatar
				</button>
				<button type="button" onClick={() => onOpenChange(false)}>
					close-avatar
				</button>
			</dialog>
		) : null,
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
	Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({ ...props }: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/authService", () => ({
	authService: {
		setAvatarSource: (...args: unknown[]) =>
			mockState.authService.setAvatarSource(...args),
		updateProfile: (...args: unknown[]) =>
			mockState.authService.updateProfile(...args),
		uploadAvatar: (...args: unknown[]) =>
			mockState.authService.uploadAvatar(...args),
	},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (selector: (state: typeof mockState) => unknown) =>
		selector(mockState),
}));

function user(overrides: Partial<MeResponse> = {}): MeResponse {
	return {
		access_token_expires_at: 1_800_000_000,
		created_at: "2026-01-01T00:00:00Z",
		email: "alice@example.test",
		email_verified: true,
		id: 7,
		preferences: {},
		profile: {
			avatar: {
				source: "upload",
				url_1024: "/avatar.png",
				url_512: "/avatar.png",
				version: 1,
			},
			display_name: "Alice",
		},
		role: "user",
		status: "active",
		storage_quota: 1024,
		storage_used: 128,
		updated_at: "2026-01-01T00:00:00Z",
		username: "alice",
		...overrides,
	};
}

describe("ProfileSettingsView", () => {
	beforeEach(() => {
		mockState.authService.setAvatarSource.mockReset();
		mockState.authService.setAvatarSource.mockResolvedValue(undefined);
		mockState.authService.updateProfile.mockReset();
		mockState.authService.updateProfile.mockResolvedValue(undefined);
		mockState.authService.uploadAvatar.mockReset();
		mockState.authService.uploadAvatar.mockResolvedValue(undefined);
		mockState.handleApiError.mockReset();
		mockState.refreshUser.mockReset();
		mockState.refreshUser.mockResolvedValue(undefined);
		mockState.toastSuccess.mockReset();
		mockState.user = user();
	});

	it("renders account fields and saves display name changes", async () => {
		render(<ProfileSettingsView />);

		expect(screen.getByTestId("avatar")).toHaveAttribute(
			"data-source",
			"upload",
		);
		expect(
			screen.getByText("settings:settings_avatar_source_upload"),
		).toBeInTheDocument();
		expect(screen.getByDisplayValue("alice")).toBeInTheDocument();
		expect(screen.getByDisplayValue("alice@example.test")).toBeInTheDocument();

		const saveButton = screen.getByRole("button", { name: "save" });
		expect(saveButton).toBeDisabled();

		fireEvent.change(screen.getByLabelText("settings:settings_display_name"), {
			target: { value: "  Alice Cooper  " },
		});
		expect(saveButton).toBeEnabled();
		fireEvent.click(saveButton);

		await waitFor(() => {
			expect(mockState.authService.updateProfile).toHaveBeenCalledWith({
				display_name: "  Alice Cooper  ",
			});
		});
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_profile_updated",
		);
	});

	it("updates avatar source and disables the current source action", async () => {
		mockState.user = user({
			profile: {
				avatar: {
					source: "none",
					url_1024: null,
					url_512: null,
					version: 1,
				},
				display_name: null,
			},
		});

		const { rerender } = render(<ProfileSettingsView />);

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_use_gravatar",
			}),
		);

		await waitFor(() => {
			expect(mockState.authService.setAvatarSource).toHaveBeenCalledWith(
				"gravatar",
			);
		});
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_avatar_source_updated",
		);

		mockState.user = user({
			profile: {
				avatar: {
					source: "gravatar",
					url_1024: "https://gravatar.example/avatar",
					url_512: "https://gravatar.example/avatar",
					version: 1,
				},
				display_name: "Alice",
			},
		});
		rerender(<ProfileSettingsView />);

		expect(
			screen.getByRole("button", {
				name: "settings:settings_use_gravatar",
			}),
		).toBeDisabled();
		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_remove_avatar",
			}),
		);

		await waitFor(() => {
			expect(mockState.authService.setAvatarSource).toHaveBeenLastCalledWith(
				"none",
			);
		});
	});

	it("opens avatar crop flow, uploads the selected file, and clears cancelled selections", async () => {
		const { rerender } = render(<ProfileSettingsView />);
		const input = screen.getByLabelText(
			"settings:settings_avatar_upload_and_crop",
		);
		const avatar = new File(["avatar"], "avatar.png", { type: "image/png" });

		fireEvent.change(input, {
			target: { files: [avatar] },
		});

		expect(
			screen.getByRole("dialog", { name: "avatar-crop" }),
		).toHaveTextContent("avatar.png");
		fireEvent.click(screen.getByRole("button", { name: "confirm-avatar" }));

		await waitFor(() => {
			expect(mockState.authService.uploadAvatar).toHaveBeenCalledWith(avatar);
		});
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_avatar_updated",
		);

		fireEvent.click(screen.getByRole("button", { name: "close-avatar" }));
		expect(screen.queryByRole("dialog", { name: "avatar-crop" })).toBeNull();

		rerender(<ProfileSettingsView />);
		fireEvent.change(input, {
			target: { files: [avatar] },
		});
		fireEvent.click(screen.getByRole("button", { name: "close-avatar" }));
		expect(screen.queryByText("avatar.png")).not.toBeInTheDocument();
	});

	it("reports profile and avatar API failures", async () => {
		const profileError = new Error("profile failed");
		const avatarError = new Error("avatar failed");
		mockState.authService.updateProfile.mockRejectedValueOnce(profileError);
		mockState.authService.uploadAvatar.mockRejectedValueOnce(avatarError);

		render(<ProfileSettingsView />);
		fireEvent.change(screen.getByLabelText("settings:settings_display_name"), {
			target: { value: "Broken" },
		});
		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(profileError);
		});

		const avatar = new File(["avatar"], "avatar.png", { type: "image/png" });
		fireEvent.change(
			screen.getByLabelText("settings:settings_avatar_upload_and_crop"),
			{
				target: { files: [avatar] },
			},
		);
		fireEvent.click(screen.getByRole("button", { name: "confirm-avatar" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(avatarError);
		});
	});
});

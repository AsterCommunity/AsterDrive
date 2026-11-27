import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TeamManageWebdavSection } from "@/components/settings/team-manage-detail/TeamManageWebdavSection";

const mockState = vi.hoisted(() => ({
	createForTeam: vi.fn(),
	fileListRoot: vi.fn(),
	handleApiError: vi.fn(),
	listForTeam: vi.fn(),
	reload: vi.fn(),
	setAccessModeForTeam: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	toggleForTeam: vi.fn(),
	useApiList: vi.fn(),
	writeText: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^(core|settings):/, ""),
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		title,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
	}) => (
		<button type="button" disabled={disabled} onClick={onClick} title={title}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: () => <span aria-hidden="true" />,
}));

vi.mock("@/components/ui/dialog-lifecycle", () => ({
	useDialogActivity: () => () => true,
}));

vi.mock("@/components/ui/table", () => ({
	TableCell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableHead: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableRow: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/webdav/WebdavAccountTable", () => ({
	WebdavAccountTable: ({
		accounts,
		onDelete,
		onSetAccessMode,
		onToggle,
	}: {
		accounts: Array<{ id: number; username: string }>;
		onDelete: (id: number) => void;
		onSetAccessMode: (id: number, accessMode: string) => void;
		onToggle: (id: number) => void;
	}) => (
		<div>
			{accounts.map((account) => (
				<div key={account.id}>
					<span>{account.username}</span>
					<button
						type="button"
						onClick={() => onToggle(account.id)}
					>{`toggle:${account.id}`}</button>
					<button
						type="button"
						onClick={() => onSetAccessMode(account.id, "read_only")}
					>{`mode:${account.id}`}</button>
					<button
						type="button"
						onClick={() => onDelete(account.id)}
					>{`delete:${account.id}`}</button>
				</div>
			))}
		</div>
	),
}));

vi.mock("@/components/webdav/WebdavCopyField", () => ({
	WebdavCopyField: () => <div />,
}));

vi.mock("@/components/webdav/WebdavCreateAccountDialog", () => ({
	WebdavCreateAccountDialog: ({
		onCreate,
		open,
	}: {
		onCreate: () => void;
		open: boolean;
	}) =>
		open ? (
			<button type="button" onClick={onCreate}>
				create
			</button>
		) : null,
}));

vi.mock("@/components/webdav/WebdavCredentialsDialog", () => ({
	WebdavCredentialsDialog: () => null,
}));

vi.mock("@/components/webdav/useWebdavAccountDialogState", () => ({
	useWebdavAccountDialogState: () => ({
		accessMode: "read_only",
		createDialogOpen: true,
		clearCredentials: vi.fn(),
		newPassword: "",
		newUsername: "team-dav-user",
		selectedFolderId: undefined,
		setAccessMode: vi.fn(),
		setCreateDialogOpen: vi.fn(),
		setCredentialsDialogOpen: vi.fn(),
		setCreating: vi.fn(),
		setNewPassword: vi.fn(),
		setNewUsername: vi.fn(),
		setSelectedFolderId: vi.fn(),
		setTestResult: vi.fn(),
		setTesting: vi.fn(),
		showCreatedCredentials: vi.fn(),
		showPassword: [],
		testing: false,
		testResult: null,
	}),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/useApiList", () => ({
	useApiList: (...args: unknown[]) => mockState.useApiList(...args),
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: (...args: unknown[]) => mockState.writeText(...args),
}));

vi.mock("@/services/fileService", () => ({
	createFileService: () => ({
		listRoot: (...args: unknown[]) => mockState.fileListRoot(...args),
	}),
}));

vi.mock("@/services/webdavAccountService", () => ({
	webdavAccountService: {
		createForTeam: (...args: unknown[]) => mockState.createForTeam(...args),
		deleteForTeam: vi.fn(),
		listForTeam: (...args: unknown[]) => mockState.listForTeam(...args),
		setAccessModeForTeam: (...args: unknown[]) =>
			mockState.setAccessModeForTeam(...args),
		test: vi.fn(),
		toggleForTeam: (...args: unknown[]) => mockState.toggleForTeam(...args),
	},
}));

function teamAccount(overrides: Record<string, unknown> = {}) {
	return {
		access_mode: "read_write",
		created_at: "2026-03-28T00:00:00Z",
		id: 21,
		is_active: true,
		root_folder_path: null,
		user_id: 7,
		username: "team-dav-user",
		...overrides,
	};
}

describe("TeamManageWebdavSection", () => {
	beforeEach(() => {
		for (const mock of Object.values(mockState)) {
			mock.mockReset();
		}
		mockState.fileListRoot.mockResolvedValue({
			folders: [{ id: 5, name: "Docs" }],
		});
		mockState.useApiList.mockReturnValue({
			items: [teamAccount()],
			loading: false,
			reload: mockState.reload,
		});
		mockState.createForTeam.mockResolvedValue({
			password: "generated-pass",
			username: "team-dav-user",
		});
		mockState.setAccessModeForTeam.mockResolvedValue(undefined);
		mockState.toggleForTeam.mockResolvedValue(undefined);
		mockState.writeText.mockResolvedValue(undefined);
	});

	it("switches a team account access mode through the team endpoint", async () => {
		render(
			<TeamManageWebdavSection
				canManageTeam
				currentUserId={7}
				teamId={3}
				webdavPrefix="/webdav"
			/>,
		);
		await screen.findByText("team-dav-user");

		fireEvent.click(screen.getByText("mode:21"));

		await waitFor(() => {
			expect(mockState.setAccessModeForTeam).toHaveBeenCalledWith(
				3,
				21,
				"read_only",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"webdav:access_mode_updated",
		);
		expect(mockState.reload).toHaveBeenCalledTimes(1);
		expect(mockState.handleApiError).not.toHaveBeenCalled();
	});

	it("reports team access mode failures through handleApiError without reloading", async () => {
		mockState.setAccessModeForTeam.mockRejectedValue(new Error("denied"));

		render(
			<TeamManageWebdavSection
				canManageTeam
				currentUserId={7}
				teamId={3}
				webdavPrefix="/webdav"
			/>,
		);
		await screen.findByText("team-dav-user");

		fireEvent.click(screen.getByText("mode:21"));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledTimes(1);
		});
		expect(mockState.reload).not.toHaveBeenCalled();
	});

	it("creates team accounts carrying the selected access mode", async () => {
		render(
			<TeamManageWebdavSection
				canManageTeam
				currentUserId={7}
				teamId={3}
				webdavPrefix="/webdav"
			/>,
		);
		await screen.findByText("team-dav-user");

		fireEvent.click(screen.getByRole("button", { name: "create" }));

		await waitFor(() => {
			expect(mockState.createForTeam).toHaveBeenCalledWith(3, {
				access_mode: "read_only",
				password: undefined,
				root_folder_id: null,
				username: "team-dav-user",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"admin:webdav_account_created",
		);
	});
});

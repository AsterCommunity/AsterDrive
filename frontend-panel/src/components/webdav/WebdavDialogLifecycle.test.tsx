import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TeamManageWebdavSection } from "@/components/settings/team-manage-detail/TeamManageWebdavSection";
import WebdavAccountsPage from "@/pages/WebdavAccountsPage";
import { DialogTestRoot } from "@/test/DialogTestRoot";
import { deferred } from "@/test/deferred";

const mock = vi.hoisted(() => ({
	create: vi.fn(),
	test: vi.fn(),
	reload: vi.fn(),
	root: vi.fn(),
	error: vi.fn(),
	success: vi.fn(),
	toastError: vi.fn(),
	copy: vi.fn(),
}));
vi.mock("react-i18next", async (original) => ({
	...(await original<typeof import("react-i18next")>()),
	useTranslation: () => ({ t: (key: string) => key.replace(/^core:/, "") }),
}));
vi.mock("sonner", () => ({
	toast: { success: mock.success, error: mock.toastError },
}));
vi.mock("@/hooks/useApiError", () => ({ handleApiError: mock.error }));
vi.mock("@/hooks/useApiList", () => ({
	useApiList: () => ({ items: [], loading: false, reload: mock.reload }),
}));
vi.mock("@/hooks/usePageTitle", () => ({ usePageTitle: vi.fn() }));
vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({ children }: { children: React.ReactNode }) => children,
}));
vi.mock("@/components/ui/dialog", () => ({
	Dialog: (props: React.ComponentProps<typeof DialogTestRoot>) => (
		<DialogTestRoot {...props} />
	),
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogFooter: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
}));
vi.mock("@/lib/clipboard", () => ({ writeTextToClipboard: mock.copy }));
vi.mock("@/services/fileService", () => ({
	createFileService: () => ({ listRoot: mock.root }),
	fileService: { listRoot: mock.root },
}));
vi.mock("@/services/webdavAccountService", () => ({
	webdavAccountService: {
		create: mock.create,
		createForTeam: (_id: number, payload: unknown) => mock.create(payload),
		test: mock.test,
		settings: async () => ({ prefix: "/webdav" }),
	},
}));

function activeDialog() {
	const node = document.querySelector<HTMLElement>(
		'[data-testid="dialog"][data-open="true"]',
	);
	if (!node) throw new Error("No active WebDAV dialog");
	return within(node);
}

function openCreate() {
	fireEvent.click(
		screen.getByRole("button", { name: "webdav:create_webdav_account" }),
	);
	fireEvent.change(activeDialog().getByLabelText("username"), {
		target: { value: "draft-user" },
	});
}

function closeAndDispose() {
	fireEvent.click(activeDialog().getByRole("button", { name: "dialog-close" }));
	for (const node of document.querySelectorAll<HTMLElement>(
		'[data-testid="dialog"][data-open="false"]',
	)) {
		fireEvent.click(
			within(node).getByRole("button", { name: "complete-close" }),
		);
	}
}

describe.each(["personal", "team"])("%s WebDAV dialog lifecycle", (scope) => {
	beforeEach(() => {
		vi.clearAllMocks();
		mock.root.mockResolvedValue({ folders: [] });
		mock.create.mockResolvedValue({
			username: "created-user",
			password: "created-secret",
		});
		mock.test.mockResolvedValue(undefined);
		mock.reload.mockResolvedValue(undefined);
		mock.copy.mockResolvedValue(undefined);
	});
	function mount() {
		return render(
			scope === "personal" ? (
				<WebdavAccountsPage />
			) : (
				<TeamManageWebdavSection
					teamId={4}
					currentUserId={1}
					canManageTeam
					webdavPrefix="/webdav"
				/>
			),
		);
	}
	async function createCredentials() {
		openCreate();
		fireEvent.click(activeDialog().getByRole("button", { name: "create" }));
		await screen.findByDisplayValue("created-secret");
	}

	it("reports active create/test failures and allows retry", async () => {
		const createError = new Error("create failed");
		mock.create.mockRejectedValueOnce(createError);
		mount();
		openCreate();
		fireEvent.click(activeDialog().getByRole("button", { name: "create" }));
		await waitFor(() => expect(mock.error).toHaveBeenCalledWith(createError));
		expect(activeDialog().getByLabelText("username")).toHaveValue("draft-user");
		fireEvent.click(activeDialog().getByRole("button", { name: "create" }));
		await screen.findByDisplayValue("created-secret");
		mock.test.mockRejectedValueOnce(new Error("connection failed"));
		fireEvent.click(
			activeDialog().getByRole("button", { name: "admin:test_connection" }),
		);
		await waitFor(() =>
			expect(mock.toastError).toHaveBeenCalledWith(
				"admin:connection_test_failed",
			),
		);
		fireEvent.click(
			activeDialog().getByRole("button", { name: "admin:test_connection" }),
		);
		await waitFor(() =>
			expect(mock.success).toHaveBeenCalledWith("admin:connection_success"),
		);
	});

	it.each(["success", "failure"])(
		"does not replace reopened create form with old %s",
		async (outcome) => {
			const pending = deferred<{ username: string; password: string }>();
			mock.create.mockReturnValueOnce(pending.promise);
			mount();
			openCreate();
			fireEvent.click(activeDialog().getByRole("button", { name: "create" }));
			closeAndDispose();
			openCreate();
			await act(async () => {
				if (outcome === "success")
					pending.resolve({ username: "old-user", password: "old-secret" });
				else pending.reject(new Error("old failure"));
				await pending.promise.catch(() => {});
			});
			if (outcome === "success") {
				expect(screen.getByDisplayValue("old-secret")).toBeInTheDocument();
				// The credential modal temporarily covers the new draft without clearing it.
				closeAndDispose();
			} else expect(screen.queryByDisplayValue("old-secret")).toBeNull();
			expect(activeDialog().getByLabelText("username")).toHaveValue(
				"draft-user",
			);
			expect(
				activeDialog().getByRole("button", { name: "create" }),
			).toBeEnabled();

			expect(mock.error).not.toHaveBeenCalled();
			if (outcome === "success") expect(mock.reload).toHaveBeenCalledTimes(1);
		},
	);

	it.each(["success", "failure"])(
		"ignores a connection %s from a closed credentials session",
		async (outcome) => {
			mount();
			await createCredentials();
			const pending = deferred<void>();
			mock.test.mockReturnValueOnce(pending.promise);
			fireEvent.click(
				activeDialog().getByRole("button", { name: "admin:test_connection" }),
			);
			closeAndDispose();
			await createCredentials();
			mock.success.mockClear();
			await act(async () => {
				if (outcome === "success") pending.resolve();
				else pending.reject(new Error("old connection"));
				await pending.promise.catch(() => {});
			});
			expect(mock.success).not.toHaveBeenCalledWith("admin:connection_success");
			expect(mock.toastError).not.toHaveBeenCalled();
			expect(
				activeDialog().getByRole("button", { name: "admin:test_connection" }),
			).toBeEnabled();
		},
	);

	it("masks closing credentials and stops copy actions before disposal", async () => {
		mount();
		await createCredentials();
		const dialog = activeDialog();
		const fields = dialog.getAllByRole("textbox");
		for (const input of fields) {
			const row = input.parentElement;
			if (row) fireEvent.click(within(row).getByRole("button"));
		}
		await waitFor(() =>
			expect(mock.copy).toHaveBeenCalledWith("created-secret"),
		);
		fireEvent.click(dialog.getByRole("button", { name: "dialog-close" }));
		const masked = screen.getByDisplayValue("••••••••");
		expect(screen.queryByDisplayValue("created-secret")).toBeNull();
		mock.copy.mockClear();
		const row = masked.parentElement;
		if (row) fireEvent.click(within(row).getByRole("button"));
		expect(mock.copy).not.toHaveBeenCalled();
	});
});

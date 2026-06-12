import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FolderPolicyDialog } from "@/components/files/FolderPolicyDialog";

const mockState = vi.hoisted(() => ({
	getFolderInfo: vi.fn(),
	handleApiError: vi.fn(),
	listAll: vi.fn(),
	onOpenChange: vi.fn(),
	onOpenChangeComplete: vi.fn(),
	onUpdated: vi.fn(),
	setPolicy: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: Record<string, unknown>) =>
			values?.name != null ? `${key}:${values.name}` : key,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		getFolderInfo: (...args: unknown[]) => mockState.getFolderInfo(...args),
	},
}));

vi.mock("@/services/adminService", () => ({
	adminFolderService: {
		setPolicy: (...args: unknown[]) => mockState.setPolicy(...args),
	},
	adminPolicyService: {
		listAll: (...args: unknown[]) => mockState.listAll(...args),
	},
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		...props
	}: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
		<button type={props.type ?? "button"} {...props}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogFooter: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<header>{children}</header>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		...props
	}: React.LabelHTMLAttributes<HTMLLabelElement>) => (
		<span {...props}>{children}</span>
	),
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		disabled,
		onValueChange,
		value,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onValueChange: (value: string) => void;
		value: string;
	}) => (
		<select
			aria-label="policy-select"
			disabled={disabled}
			value={value}
			onChange={(event) => onValueChange(event.target.value)}
		>
			{children}
		</select>
	),
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<>{children}</>
	),
	SelectItem: ({
		children,
		value,
	}: {
		children: React.ReactNode;
		value: string;
	}) => <option value={value}>{children}</option>,
	SelectTrigger: ({ children }: { children: React.ReactNode }) => (
		<>{children}</>
	),
	SelectValue: ({ children }: { children?: React.ReactNode }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/lib/utils", () => ({
	cn: (...values: Array<string | false | null | undefined>) =>
		values.filter(Boolean).join(" "),
}));

function folderInfo(policyId: number | null = null) {
	return {
		created_at: "2026-06-12T00:00:00Z",
		created_by_username: "alice",
		id: 12,
		is_locked: false,
		name: "Projects",
		policy_id: policyId,
		tags: [],
		updated_at: "2026-06-12T00:00:00Z",
	};
}

function policy(id: number, name: string, driverType = "local") {
	return {
		allowed_types: [],
		base_path: "",
		bucket: "",
		chunk_size: 1024,
		created_at: "2026-06-12T00:00:00Z",
		driver_type: driverType,
		endpoint: "",
		id,
		is_default: id === 1,
		max_file_size: 1024,
		name,
		options: {},
		updated_at: "2026-06-12T00:00:00Z",
	};
}

function renderDialog(folder = { id: 12, name: "Projects" }) {
	render(
		<FolderPolicyDialog
			open
			onOpenChange={mockState.onOpenChange}
			onOpenChangeComplete={mockState.onOpenChangeComplete}
			folder={folder as never}
			onUpdated={mockState.onUpdated}
		/>,
	);
}

describe("FolderPolicyDialog", () => {
	beforeEach(() => {
		mockState.getFolderInfo.mockReset();
		mockState.handleApiError.mockReset();
		mockState.listAll.mockReset();
		mockState.onOpenChange.mockReset();
		mockState.onOpenChangeComplete.mockReset();
		mockState.onUpdated.mockReset();
		mockState.setPolicy.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.getFolderInfo.mockResolvedValue(folderInfo(null));
		mockState.listAll.mockResolvedValue([
			policy(1, "Primary"),
			policy(2, "Cold"),
		]);
		mockState.setPolicy.mockResolvedValue(folderInfo(null));
		mockState.onUpdated.mockResolvedValue(undefined);
	});

	it("loads folder details and storage policies when opened", async () => {
		mockState.getFolderInfo.mockResolvedValue(folderInfo(2));

		renderDialog();

		expect(mockState.getFolderInfo).toHaveBeenCalledWith(12);
		expect(mockState.listAll).toHaveBeenCalledTimes(1);
		await screen.findByText("folder_policy_current_named:Cold");
		expect(screen.getByRole("combobox")).toHaveValue("2");
		expect(screen.getByText("Cold (#2)")).toBeInTheDocument();
		expect(screen.getByText(/Primary/)).toBeInTheDocument();
	});

	it("renders the inherit option label instead of the raw sentinel value", async () => {
		renderDialog();

		await screen.findByText("folder_policy_current_inherit");
		expect(screen.getAllByText("folder_policy_inherit")).toHaveLength(2);
		expect(screen.queryByText("__inherit__")).not.toBeInTheDocument();
	});

	it("saves an explicit folder policy and closes the dialog", async () => {
		mockState.setPolicy.mockResolvedValue(folderInfo(2));
		renderDialog();

		await waitFor(() => expect(screen.getByRole("combobox")).toBeEnabled());
		fireEvent.change(screen.getByRole("combobox"), {
			target: { value: "2" },
		});
		fireEvent.click(screen.getByRole("button", { name: /folder_policy_save/ }));

		await waitFor(() =>
			expect(mockState.setPolicy).toHaveBeenCalledWith(12, { policy_id: 2 }),
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"folder_policy_updated",
		);
		expect(mockState.onUpdated).toHaveBeenCalledTimes(1);
		expect(mockState.onOpenChange).toHaveBeenCalledWith(false);
	});

	it("clears the explicit folder policy binding", async () => {
		mockState.getFolderInfo.mockResolvedValue(folderInfo(1));
		renderDialog();

		await waitFor(() => expect(screen.getByRole("combobox")).toHaveValue("1"));
		fireEvent.change(screen.getByRole("combobox"), {
			target: { value: "__inherit__" },
		});
		fireEvent.click(screen.getByRole("button", { name: /folder_policy_save/ }));

		await waitFor(() =>
			expect(mockState.setPolicy).toHaveBeenCalledWith(12, { policy_id: null }),
		);
	});

	it("reports load failures without enabling submit", async () => {
		const error = new Error("boom");
		mockState.getFolderInfo.mockRejectedValue(error);

		renderDialog();

		await waitFor(() =>
			expect(mockState.handleApiError).toHaveBeenCalledWith(error),
		);
		expect(
			screen.getByRole("button", { name: /folder_policy_save/ }),
		).toBeDisabled();
	});
});

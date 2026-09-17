import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FolderIconDialog } from "./FolderIconDialog";

const mocks = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	onOpenChange: vi.fn(),
	onUpdated: vi.fn(),
	setFolderIcon: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("sonner", () => ({ toast: { success: mocks.toastSuccess } }));
vi.mock("@/hooks/useApiError", () => ({
	handleApiError: mocks.handleApiError,
}));
vi.mock("@/services/fileService", () => ({
	fileService: { setFolderIcon: mocks.setFolderIcon },
}));

const folder = {
	id: 7,
	name: "Docs",
	icon: { kind: "default" as const },
	is_shared: false,
	lock_state: { state: "unlocked" as const },
	tags: [],
	updated_at: "2026-09-17T00:00:00Z",
};

describe("FolderIconDialog", () => {
	beforeEach(() => {
		for (const mock of Object.values(mocks)) mock.mockReset();
		mocks.setFolderIcon.mockResolvedValue({ ...folder });
	});

	it("saves a selected builtin icon and refreshes in place", async () => {
		render(
			<FolderIconDialog
				open
				onOpenChange={mocks.onOpenChange}
				folder={folder}
				onUpdated={mocks.onUpdated}
			/>,
		);
		fireEvent.click(
			screen.getByRole("button", { name: "folder_icon_mode_builtin" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "folder_icon_builtin_images" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "folder_icon_save" }));

		await waitFor(() =>
			expect(mocks.setFolderIcon).toHaveBeenCalledWith(7, {
				kind: "builtin",
				key: "images",
			}),
		);
		expect(mocks.onOpenChange).toHaveBeenCalledWith(false);
		expect(mocks.onUpdated).toHaveBeenCalledOnce();
	});

	it("submits emoji and can restore the default", async () => {
		const { rerender } = render(
			<FolderIconDialog
				open
				onOpenChange={mocks.onOpenChange}
				folder={folder}
			/>,
		);
		fireEvent.click(
			screen.getByRole("button", { name: "folder_icon_mode_emoji" }),
		);
		fireEvent.change(screen.getByLabelText("folder_icon_emoji_input"), {
			target: { value: "📚" },
		});
		fireEvent.click(screen.getByRole("button", { name: "folder_icon_save" }));
		await waitFor(() =>
			expect(mocks.setFolderIcon).toHaveBeenCalledWith(7, {
				kind: "emoji",
				value: "📚",
			}),
		);

		mocks.setFolderIcon.mockClear();
		rerender(
			<FolderIconDialog
				open
				onOpenChange={mocks.onOpenChange}
				folder={{ ...folder, icon: { kind: "emoji", value: "📚" } }}
			/>,
		);
		fireEvent.click(
			screen.getByRole("button", { name: "folder_icon_mode_default" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "folder_icon_save" }));
		await waitFor(() =>
			expect(mocks.setFolderIcon).toHaveBeenCalledWith(7, { kind: "default" }),
		);
	});

	it("keeps the dialog open and re-enables save after an API error", async () => {
		mocks.setFolderIcon.mockRejectedValue(new Error("invalid emoji"));
		render(
			<FolderIconDialog
				open
				onOpenChange={mocks.onOpenChange}
				folder={folder}
			/>,
		);
		fireEvent.click(screen.getByRole("button", { name: "folder_icon_save" }));
		await waitFor(() => expect(mocks.handleApiError).toHaveBeenCalledOnce());
		expect(mocks.onOpenChange).not.toHaveBeenCalled();
		expect(
			screen.getByRole("button", { name: "folder_icon_save" }),
		).toBeEnabled();
	});
});

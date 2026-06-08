import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	CurrentFolderContextMenuContent,
	CurrentFolderDropdownMenuContent,
} from "@/pages/file-browser/CurrentFolderActionsMenu";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^core:/, ""),
	}),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span aria-hidden>{name}</span>,
}));

vi.mock("@/components/ui/context-menu", () => ({
	ContextMenuContent: ({ children }: { children: React.ReactNode }) => (
		<div data-testid="context-content">{children}</div>
	),
	ContextMenuItem: ({
		children,
		disabled,
		onClick,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
	ContextMenuSeparator: () => <hr data-testid="context-separator" />,
}));

vi.mock("@/components/ui/dropdown-menu", () => ({
	DropdownMenuContent: ({ children }: { children: React.ReactNode }) => (
		<div data-testid="dropdown-content">{children}</div>
	),
	DropdownMenuItem: ({
		children,
		disabled,
		onClick,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
	DropdownMenuSeparator: () => <hr data-testid="dropdown-separator" />,
}));

function props(
	overrides: Partial<
		React.ComponentProps<typeof CurrentFolderContextMenuContent>
	> = {},
) {
	return {
		uploadReady: true,
		onCreateFile: vi.fn(),
		onCreateFolder: vi.fn(),
		onManageTagLibrary: vi.fn(),
		onOfflineDownload: vi.fn(),
		onRefresh: vi.fn(),
		onTriggerFileUpload: vi.fn(),
		onTriggerFolderUpload: vi.fn(),
		...overrides,
	} satisfies React.ComponentProps<typeof CurrentFolderContextMenuContent>;
}

describe("CurrentFolderActionsMenu", () => {
	it("renders context menu actions and invokes the tag library callback", () => {
		const menuProps = props();

		render(<CurrentFolderContextMenuContent {...menuProps} />);

		fireEvent.click(screen.getByRole("button", { name: "upload_file" }));
		fireEvent.click(screen.getByRole("button", { name: "upload_folder" }));
		fireEvent.click(screen.getByRole("button", { name: "new_folder" }));
		fireEvent.click(screen.getByRole("button", { name: "new_file" }));
		fireEvent.click(
			screen.getByRole("button", { name: "tasks:offline_download_action" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "tag_library_manage" }));
		fireEvent.click(screen.getByRole("button", { name: "refresh" }));

		expect(menuProps.onTriggerFileUpload).toHaveBeenCalledTimes(1);
		expect(menuProps.onTriggerFolderUpload).toHaveBeenCalledTimes(1);
		expect(menuProps.onCreateFolder).toHaveBeenCalledTimes(1);
		expect(menuProps.onCreateFile).toHaveBeenCalledTimes(1);
		expect(menuProps.onOfflineDownload).toHaveBeenCalledTimes(1);
		expect(menuProps.onManageTagLibrary).toHaveBeenCalledTimes(1);
		expect(menuProps.onRefresh).toHaveBeenCalledTimes(1);
		expect(screen.getAllByTestId("context-separator")).toHaveLength(3);
	});

	it("renders dropdown actions with disabled upload entries when upload is not ready", () => {
		const menuProps = props({ uploadReady: false });

		render(<CurrentFolderDropdownMenuContent {...menuProps} />);

		expect(screen.getByRole("button", { name: "upload_file" })).toBeDisabled();
		expect(
			screen.getByRole("button", { name: "upload_folder" }),
		).toBeDisabled();
		fireEvent.click(screen.getByRole("button", { name: "tag_library_manage" }));

		expect(menuProps.onTriggerFileUpload).not.toHaveBeenCalled();
		expect(menuProps.onTriggerFolderUpload).not.toHaveBeenCalled();
		expect(menuProps.onManageTagLibrary).toHaveBeenCalledTimes(1);
		expect(screen.getAllByTestId("dropdown-separator")).toHaveLength(3);
	});

	it("omits tag library management when no callback is provided", () => {
		render(
			<CurrentFolderContextMenuContent
				{...props({ onManageTagLibrary: undefined })}
			/>,
		);

		expect(
			screen.queryByRole("button", { name: "tag_library_manage" }),
		).not.toBeInTheDocument();
		expect(screen.getAllByTestId("context-separator")).toHaveLength(2);
	});
});

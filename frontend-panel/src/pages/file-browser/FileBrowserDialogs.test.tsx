import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FileBrowserDialogs } from "./FileBrowserDialogs";

vi.mock("@/components/files/FilePreview", () => ({
	FilePreview: ({
		file,
		imageNavigation,
	}: {
		file: { name: string };
		imageNavigation?: {
			nextFile?: { name: string };
			previousFile?: { name: string };
		};
	}) => (
		<div
			data-testid="file-preview"
			data-name={file.name}
			data-has-image-navigation={String(Boolean(imageNavigation))}
			data-next-image={imageNavigation?.nextFile?.name ?? ""}
			data-previous-image={imageNavigation?.previousFile?.name ?? ""}
		/>
	),
}));

vi.mock("@/hooks/useRetainedDialogValue", () => ({
	useRetainedDialogValue: <T,>(value: T) => ({
		retainedValue: value,
		handleOpenChangeComplete: vi.fn(),
	}),
}));

vi.mock("@/pages/file-browser/fileBrowserLazy", () => {
	const Dialog = ({ open }: { open: boolean }) =>
		open ? <div data-testid="lazy-dialog" /> : null;
	const lazy = Object.assign(Dialog, { preload: vi.fn() });
	return {
		ArchiveTaskNameDialog: lazy,
		BatchTargetFolderDialog: lazy,
		CreateFileDialog: lazy,
		CreateFolderDialog: lazy,
		FolderPolicyDialog: lazy,
		OfflineDownloadDialog: lazy,
		RenameDialog: lazy,
		ShareDialog: lazy,
		VersionHistoryDialog: lazy,
	};
});

vi.mock("@/services/fileService", () => ({
	fileService: {
		createPreviewLink: vi.fn(),
		getArchivePreview: vi.fn(),
		createWopiSession: vi.fn(),
	},
}));

const previewFile = {
	id: 7,
	name: "current.png",
	mime_type: "image/png",
	size: 10,
};
const previousFile = {
	...previewFile,
	id: 6,
	name: "previous.png",
};
const nextFile = {
	...previewFile,
	id: 8,
	name: "next.png",
};

function renderDialogs(
	overrides: Partial<React.ComponentProps<typeof FileBrowserDialogs>> = {},
) {
	const props: React.ComponentProps<typeof FileBrowserDialogs> = {
		archiveTaskTarget: null,
		breadcrumb: [],
		copyTarget: null,
		createFileOpen: false,
		createFolderOpen: false,
		currentFolderId: null,
		folderPolicyTarget: null,
		moveTarget: null,
		offlineDownloadOpen: false,
		previewState: { file: previewFile as never, openMode: "auto" },
		renameTarget: null,
		shareTarget: null,
		versionTarget: null,
		onArchiveTaskClose: vi.fn(),
		onArchiveTaskSubmit: vi.fn(),
		onCopyClose: vi.fn(),
		onCopyConfirm: vi.fn(),
		onCreateFileOpenChange: vi.fn(),
		onCreateFolderOpenChange: vi.fn(),
		onFolderPolicyClose: vi.fn(),
		onMoveClose: vi.fn(),
		onMoveConfirm: vi.fn(),
		onOfflineDownloadOpenChange: vi.fn(),
		onPreviewClose: vi.fn(),
		onPreviewFileUpdated: vi.fn(),
		onRenameClose: vi.fn(),
		onShareClose: vi.fn(),
		onVersionClose: vi.fn(),
		onVersionRestored: vi.fn(),
		...overrides,
	};

	return render(<FileBrowserDialogs {...props} />);
}

function renderDialogsElement(
	overrides: Partial<React.ComponentProps<typeof FileBrowserDialogs>> = {},
) {
	const props: React.ComponentProps<typeof FileBrowserDialogs> = {
		archiveTaskTarget: null,
		breadcrumb: [],
		copyTarget: null,
		createFileOpen: false,
		createFolderOpen: false,
		currentFolderId: null,
		folderPolicyTarget: null,
		moveTarget: null,
		offlineDownloadOpen: false,
		previewState: { file: previewFile as never, openMode: "auto" },
		renameTarget: null,
		shareTarget: null,
		versionTarget: null,
		onArchiveTaskClose: vi.fn(),
		onArchiveTaskSubmit: vi.fn(),
		onCopyClose: vi.fn(),
		onCopyConfirm: vi.fn(),
		onCreateFileOpenChange: vi.fn(),
		onCreateFolderOpenChange: vi.fn(),
		onFolderPolicyClose: vi.fn(),
		onMoveClose: vi.fn(),
		onMoveConfirm: vi.fn(),
		onOfflineDownloadOpenChange: vi.fn(),
		onPreviewClose: vi.fn(),
		onPreviewFileUpdated: vi.fn(),
		onRenameClose: vi.fn(),
		onShareClose: vi.fn(),
		onVersionClose: vi.fn(),
		onVersionRestored: vi.fn(),
		...overrides,
	};

	return <FileBrowserDialogs {...props} />;
}

describe("FileBrowserDialogs", () => {
	it("forwards image navigation only when navigation data and callback are present", () => {
		const navigation = {
			previousFile: previousFile as never,
			nextFile: nextFile as never,
		};

		const { rerender } = renderDialogs({
			previewImageNavigation: navigation,
		});

		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-has-image-navigation",
			"false",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-previous-image",
			"",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-next-image",
			"",
		);

		rerender(
			renderDialogsElement({
				previewImageNavigation: navigation,
				onPreviewNavigate: vi.fn(),
			}),
		);

		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-has-image-navigation",
			"true",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-previous-image",
			"previous.png",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-next-image",
			"next.png",
		);
	});
});

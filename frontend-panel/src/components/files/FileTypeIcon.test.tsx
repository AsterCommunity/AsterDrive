import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileTypeIcon } from "@/components/files/FileTypeIcon";

const mockState = vi.hoisted(() => ({
	fileTypeInfo: {
		category: "text",
		icon: "FileText",
		color: "text-blue-500",
	},
}));

vi.mock("@/components/files/preview/file-capabilities", () => ({
	getFileTypeInfo: vi.fn(() => mockState.fileTypeInfo),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name, className }: { name: string; className?: string }) => (
		<span data-testid="icon" data-name={name} className={className} />
	),
}));

const mockHasLanguageIcon = vi.fn(() => false);
const mockIsIconMapLoaded = vi.fn(() => true);
const mockLoadLanguageIcons = vi.fn(() => Promise.resolve());

vi.mock("@/components/ui/language-icon", () => ({
	hasLanguageIcon: (name: string) => mockHasLanguageIcon(name),
	isIconMapLoaded: () => mockIsIconMapLoaded(),
	loadLanguageIcons: () => mockLoadLanguageIcons(),
	LanguageIcon: ({ name, className }: { name: string; className?: string }) => (
		<span data-testid="language-icon" data-name={name} className={className} />
	),
}));

beforeEach(() => {
	mockHasLanguageIcon.mockReset();
	mockHasLanguageIcon.mockReturnValue(false);
	mockIsIconMapLoaded.mockReset();
	mockIsIconMapLoaded.mockReturnValue(true);
	mockLoadLanguageIcons.mockClear();
	mockState.fileTypeInfo = {
		category: "text",
		icon: "FileText",
		color: "text-blue-500",
	};
});

describe("FileTypeIcon", () => {
	it("renders the icon and color returned by file type detection", () => {
		render(
			<FileTypeIcon
				mimeType="application/pdf"
				fileName="manual.pdf"
				className="size-4"
			/>,
		);

		expect(screen.getByTestId("icon")).toHaveAttribute("data-name", "FileText");
		expect(screen.getByTestId("icon")).toHaveClass("text-blue-500", "size-4");
	});

	it("renders a language icon when the icon map is loaded and the file matches", () => {
		mockHasLanguageIcon.mockReturnValue(true);

		render(
			<FileTypeIcon
				mimeType="text/plain"
				fileName="main.ts"
				className="size-4"
			/>,
		);

		expect(screen.getByTestId("language-icon")).toHaveAttribute(
			"data-name",
			"main.ts",
		);
		expect(screen.getByTestId("language-icon")).toHaveClass("size-4");
		expect(screen.queryByTestId("icon")).not.toBeInTheDocument();
	});

	it("does not replace image file icons with language icons", () => {
		mockHasLanguageIcon.mockReturnValue(true);
		mockState.fileTypeInfo = {
			category: "image",
			icon: "FileImage",
			color: "text-sky-500",
		};

		render(
			<FileTypeIcon
				mimeType="image/svg+xml"
				fileName="diagram.svg"
				className="size-4"
			/>,
		);

		expect(screen.getByTestId("icon")).toHaveAttribute(
			"data-name",
			"FileImage",
		);
		expect(screen.queryByTestId("language-icon")).not.toBeInTheDocument();
	});

	it("loads the language icon map before replacing code category icons", async () => {
		mockIsIconMapLoaded.mockReturnValue(false);
		mockHasLanguageIcon.mockReturnValue(true);

		render(
			<FileTypeIcon
				mimeType="text/plain"
				fileName="index.ts"
				fileCategory="code"
				className="size-4"
			/>,
		);

		expect(screen.getByTestId("icon")).toHaveAttribute("data-name", "FileCode");
		expect(mockLoadLanguageIcons).toHaveBeenCalledTimes(1);

		expect(await screen.findByTestId("language-icon")).toHaveAttribute(
			"data-name",
			"index.ts",
		);
	});

	it("does not update state after unmounting while language icons are loading", async () => {
		let resolveLoad: (() => void) | undefined;
		mockIsIconMapLoaded.mockReturnValue(false);
		mockLoadLanguageIcons.mockReturnValue(
			new Promise<void>((resolve) => {
				resolveLoad = resolve;
			}),
		);

		const { unmount } = render(
			<FileTypeIcon mimeType="text/plain" fileName="index.ts" />,
		);

		unmount();
		resolveLoad?.();

		await waitFor(() => {
			expect(mockLoadLanguageIcons).toHaveBeenCalledTimes(1);
		});
		expect(screen.queryByTestId("language-icon")).not.toBeInTheDocument();
	});
});

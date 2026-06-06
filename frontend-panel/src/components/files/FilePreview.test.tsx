import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FilePreview } from "@/components/files/FilePreview";

vi.mock("@/components/files/preview/FilePreviewDialog", () => ({
	FilePreviewDialog: ({
		open,
		file,
		downloadPath,
		imagePreviewPath,
		editable,
		previewLinkFactory,
		mediaStreamLinkFactory,
		wopiSessionFactory,
		imageNavigation,
	}: {
		open: boolean;
		file: { name: string };
		downloadPath?: string;
		imagePreviewPath?: string;
		editable?: boolean;
		previewLinkFactory?: () => Promise<unknown>;
		mediaStreamLinkFactory?: () => Promise<unknown>;
		wopiSessionFactory?: (appKey: string) => Promise<unknown>;
		imageNavigation?: { onNavigate: (file: unknown) => void };
	}) => (
		<div
			data-testid="preview-dialog"
			data-open={String(open)}
			data-file-name={file.name}
			data-download-path={downloadPath ?? ""}
			data-image-preview-path={imagePreviewPath ?? ""}
			data-editable={String(Boolean(editable))}
			data-has-preview-link-factory={String(Boolean(previewLinkFactory))}
			data-has-media-stream-link-factory={String(
				Boolean(mediaStreamLinkFactory),
			)}
			data-has-wopi-session-factory={String(Boolean(wopiSessionFactory))}
			data-has-image-navigation={String(Boolean(imageNavigation))}
		/>
	),
}));

describe("FilePreview", () => {
	it("forwards all props to the preview dialog", () => {
		render(
			<FilePreview
				file={{ id: 7, name: "report.pdf" } as never}
				open
				onClose={vi.fn()}
				onFileUpdated={vi.fn()}
				downloadPath="/files/7/download"
				imagePreviewPath="/files/7/image-preview"
				editable
				previewLinkFactory={async () => ({})}
				mediaStreamLinkFactory={async () => ({})}
				wopiSessionFactory={async () => ({})}
				imageNavigation={{
					previousFile: { id: 6, name: "previous.png" } as never,
					nextFile: { id: 8, name: "next.png" } as never,
					onNavigate: vi.fn(),
				}}
			/>,
		);

		expect(screen.getByTestId("preview-dialog")).toHaveAttribute(
			"data-open",
			"true",
		);
		expect(screen.getByTestId("preview-dialog")).toHaveAttribute(
			"data-file-name",
			"report.pdf",
		);
		expect(screen.getByTestId("preview-dialog")).toHaveAttribute(
			"data-download-path",
			"/files/7/download",
		);
		expect(screen.getByTestId("preview-dialog")).toHaveAttribute(
			"data-image-preview-path",
			"/files/7/image-preview",
		);
		expect(screen.getByTestId("preview-dialog")).toHaveAttribute(
			"data-editable",
			"true",
		);
		expect(screen.getByTestId("preview-dialog")).toHaveAttribute(
			"data-has-preview-link-factory",
			"true",
		);
		expect(screen.getByTestId("preview-dialog")).toHaveAttribute(
			"data-has-media-stream-link-factory",
			"true",
		);
		expect(screen.getByTestId("preview-dialog")).toHaveAttribute(
			"data-has-wopi-session-factory",
			"true",
		);
		expect(screen.getByTestId("preview-dialog")).toHaveAttribute(
			"data-has-image-navigation",
			"true",
		);
	});
});

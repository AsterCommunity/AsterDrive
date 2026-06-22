import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FilePreview } from "@/components/files/FilePreview";

vi.mock("@/components/files/preview/FilePreviewDialog", () => ({
	FilePreviewDialog: ({
		open,
		file,
		editable,
		resources,
		imageNavigation,
	}: {
		open: boolean;
		file: { name: string };
		editable?: boolean;
		resources?: {
			paths: { download: string; imagePreview?: string };
			actions?: {
				createExternalPreviewLink?: () => Promise<unknown>;
				createMediaStreamSession?: () => Promise<unknown>;
				launchWopiSession?: (appKey: string) => Promise<unknown>;
			};
		};
		imageNavigation?: { onNavigate: (file: unknown) => void };
	}) => (
		<div
			data-testid="preview-dialog"
			data-open={String(open)}
			data-file-name={file.name}
			data-download-path={resources?.paths.download ?? ""}
			data-image-preview-path={resources?.paths.imagePreview ?? ""}
			data-editable={String(Boolean(editable))}
			data-has-preview-link-factory={String(
				Boolean(resources?.actions?.createExternalPreviewLink),
			)}
			data-has-media-stream-link-factory={String(
				Boolean(resources?.actions?.createMediaStreamSession),
			)}
			data-has-wopi-session-factory={String(
				Boolean(resources?.actions?.launchWopiSession),
			)}
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
				editable
				resources={{
					scope: "personal",
					paths: {
						download: "/files/7/download",
						imagePreview: "/files/7/image-preview",
					},
					resolve: vi.fn(),
					actions: {
						createExternalPreviewLink: async () => ({}) as never,
						createMediaStreamSession: async () => ({}) as never,
						launchWopiSession: async () => ({}) as never,
					},
				}}
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

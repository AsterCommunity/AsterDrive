import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AdminFileDetail } from "@/types/api";
import { AdminFileDownloads } from "./AdminFileDownloads";

const mocks = vi.hoisted(() => ({ download: vi.fn(), error: vi.fn() }));
vi.mock("@/services/adminContentDownload", () => ({
	startAdminContentDownload: mocks.download,
}));
vi.mock("@/hooks/useApiError", () => ({ handleApiError: mocks.error }));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("./AdminContentAccessGate", () => ({
	AdminContentAccessGate: ({
		children,
		scopeKind,
		targetId,
	}: {
		children: (id: string, end: () => void) => ReactNode;
		scopeKind: string;
		targetId: number;
	}) => (
		<div data-testid="scope">
			{scopeKind}:{targetId}
			{children("context", () => {})}
		</div>
	),
}));
function file(overrides: Partial<AdminFileDetail> = {}): AdminFileDetail {
	const blob = {
		id: 3,
		hash: "content",
		size: 10,
		policy_id: 1,
		storage_path: "objects/content",
	};
	return {
		id: 7,
		name: "file.txt",
		blob_id: 3,
		size: 10,
		mime_type: "text/plain",
		extension: "txt",
		compound_extension: null,
		file_category: "text",
		folder_id: null,
		team_id: null,
		owner_user_id: 8,
		created_by_user_id: 8,
		created_by_username: "owner",
		created_by: null,
		created_at: "2026-10-30T00:00:00Z",
		updated_at: "2026-10-30T00:00:00Z",
		deleted_at: null,
		lock_state: { state: "unlocked" },
		blob,
		versions: [
			{
				id: 44,
				file_id: 7,
				blob_id: 3,
				version: 2,
				size: 10,
				created_at: "2026-10-30T00:00:00Z",
				blob,
			},
		],
		...overrides,
	};
}
describe("administrator file downloads", () => {
	beforeEach(() => {
		vi.resetAllMocks();
		mocks.download.mockResolvedValue(undefined);
	});
	it.each([undefined, 44])(
		"initializes the logical file and selected revision %s",
		async (revisionId) => {
			render(<AdminFileDownloads file={file()} revisionId={revisionId} />);
			expect(mocks.download).not.toHaveBeenCalled();
			fireEvent.click(
				screen.getByRole("button", { name: "content_access_download" }),
			);
			expect(screen.getByTestId("scope")).toHaveTextContent("personal:8");
			fireEvent.click(
				screen.getByRole("button", {
					name: revisionId
						? "content_download_revision"
						: "content_download_current",
				}),
			);
			await waitFor(() =>
				expect(mocks.download).toHaveBeenCalledWith("context", 7, revisionId),
			);
		},
	);
	it("derives team ownership from the file and rejects nonexistent revisions", () => {
		const view = render(
			<AdminFileDownloads file={file({ team_id: 12, owner_user_id: null })} />,
		);
		fireEvent.click(
			screen.getByRole("button", { name: "content_access_download" }),
		);
		expect(screen.getByTestId("scope")).toHaveTextContent("team:12");
		view.rerender(<AdminFileDownloads file={file()} revisionId={999} />);
		expect(screen.getByRole("alert")).toHaveTextContent(
			"content_revision_unavailable",
		);
		expect(mocks.download).not.toHaveBeenCalled();
	});
	it("surfaces initialization failures and restores the download action", async () => {
		mocks.download.mockRejectedValue(new Error("audit failed"));
		render(<AdminFileDownloads file={file()} />);
		fireEvent.click(
			screen.getByRole("button", { name: "content_access_download" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "content_download_current" }),
		);
		await waitFor(() => expect(mocks.error).toHaveBeenCalledOnce());
		expect(
			screen.getByRole("button", { name: "content_download_current" }),
		).not.toBeDisabled();
	});
});

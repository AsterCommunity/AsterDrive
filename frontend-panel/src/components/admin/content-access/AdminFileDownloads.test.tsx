import { fireEvent, render, screen, waitFor } from "@testing-library/react";
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
		"downloads revision %s directly without a reason form or extra activation",
		async (revisionId) => {
			render(<AdminFileDownloads file={file()} revisionId={revisionId} />);
			expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
			expect(
				screen.queryByRole("button", { name: "content_access_download" }),
			).not.toBeInTheDocument();
			fireEvent.click(
				screen.getByRole("button", {
					name: revisionId
						? "content_download_revision"
						: "content_download_current",
				}),
			);
			await waitFor(() =>
				expect(mocks.download).toHaveBeenCalledExactlyOnceWith(
					undefined,
					7,
					revisionId,
				),
			);
		},
	);
	it("supports team-owned files and rejects missing revisions and ownerless files", async () => {
		const view = render(
			<AdminFileDownloads file={file({ team_id: 12, owner_user_id: null })} />,
		);
		fireEvent.click(
			screen.getByRole("button", { name: "content_download_current" }),
		);
		await waitFor(() => expect(mocks.download).toHaveBeenCalledOnce());
		view.rerender(<AdminFileDownloads file={file()} revisionId={999} />);
		expect(screen.getByRole("alert")).toHaveTextContent(
			"content_revision_unavailable",
		);
		view.rerender(
			<AdminFileDownloads
				file={file({ owner_user_id: null, team_id: null })}
			/>,
		);
		expect(screen.getByRole("alert")).toHaveTextContent(
			"content_access_failed",
		);
		expect(mocks.download).toHaveBeenCalledOnce();
	});
	it("prevents duplicate initialization while busy and restores retry after failure", async () => {
		let fail: (error: Error) => void = () => {};
		mocks.download.mockReturnValueOnce(
			new Promise((_resolve, reject) => {
				fail = reject;
			}),
		);
		render(<AdminFileDownloads file={file()} />);
		const button = screen.getByRole("button", {
			name: "content_download_current",
		});
		fireEvent.click(button);
		fireEvent.click(button);
		expect(mocks.download).toHaveBeenCalledOnce();
		expect(button).toBeDisabled();
		fail(new Error("audit failed"));
		await waitFor(() => expect(button).toBeEnabled());
		expect(mocks.error).toHaveBeenCalledOnce();
		fireEvent.click(button);
		await waitFor(() => expect(mocks.download).toHaveBeenCalledTimes(2));
	});
});

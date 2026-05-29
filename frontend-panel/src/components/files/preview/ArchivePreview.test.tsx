import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ArchivePreview } from "@/components/files/preview/ArchivePreview";
import { ApiError, ApiPendingError } from "@/services/http";
import type { ArchivePreviewManifest } from "@/types/api";
import { ApiSubcode, ErrorCode } from "@/types/api-helpers";

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
	formatDateTime: (value: string) => `time:${value}`,
	formatNumber: (value: number) => `num:${value}`,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, opts?: Record<string, string>) =>
			opts?.date ? `${key}:${opts.date}` : key,
	}),
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		onValueChange,
		value,
	}: {
		children: React.ReactNode;
		onValueChange?: (value: string) => void;
		value?: string;
	}) => (
		<div data-value={value}>
			{children}
			<button type="button" onClick={() => onValueChange?.("gb18030")}>
				select-gb18030
			</button>
		</div>
	),
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({
		children,
		value,
	}: {
		children: React.ReactNode;
		value: string;
	}) => <div data-value={value}>{children}</div>,
	SelectTrigger: ({
		"aria-label": ariaLabel,
		children,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
	}) => (
		<button type="button" aria-label={ariaLabel}>
			{children}
		</button>
	),
	SelectValue: () => <span>select-value</span>,
}));

const manifest: ArchivePreviewManifest = {
	schema_version: 2,
	format: "zip",
	source_blob_id: 10,
	source_hash: "hash",
	generated_at: "2026-01-02T03:04:05Z",
	entry_count: 3,
	file_count: 2,
	directory_count: 1,
	total_uncompressed_size: 12,
	truncated: true,
	extract_compatibility: {
		supported: true,
		reason: null,
	},
	entries: [
		{
			path: "docs",
			name: "docs",
			parent: null,
			kind: "directory",
			size: 0,
			compressed_size: 0,
			modified_at: null,
		},
		{
			path: "docs/readme.txt",
			name: "readme.txt",
			parent: "docs",
			kind: "file",
			size: 5,
			compressed_size: 5,
			modified_at: "2026-01-02T03:04:05",
		},
		{
			path: "image.bin",
			name: "image.bin",
			parent: null,
			kind: "file",
			size: 7,
			compressed_size: 7,
			modified_at: null,
		},
	],
};

const sevenZipManifest: ArchivePreviewManifest = {
	...manifest,
	format: "7z",
};

describe("ArchivePreview", () => {
	it("shows unavailable state without a loader", () => {
		render(<ArchivePreview />);

		expect(screen.getByText("preview_not_available")).toBeInTheDocument();
		expect(screen.getByText("preview_not_available_desc")).toBeInTheDocument();
	});

	it("loads and filters archive entries", async () => {
		const loadManifest = vi.fn(async () => manifest);
		render(<ArchivePreview loadManifest={loadManifest} />);

		expect(await screen.findByText("docs")).toBeInTheDocument();
		expect(loadManifest).toHaveBeenCalledWith(
			expect.objectContaining({ filenameEncoding: "auto" }),
		);
		expect(screen.getByText("archive_preview_truncated")).toBeInTheDocument();
		const itemSummary = screen.getByText("archive_preview_items").parentElement;
		expect(itemSummary).not.toBeNull();
		expect(
			within(itemSummary as HTMLElement).getByText("num:3"),
		).toBeInTheDocument();
		expect(screen.getByText("archive_preview_entries")).toBeInTheDocument();
		expect(screen.getByText("bytes:12")).toBeInTheDocument();
		expect(screen.queryByText("readme.txt")).not.toBeInTheDocument();

		fireEvent.click(screen.getByText("docs"));

		expect(screen.getByText("readme.txt")).toBeInTheDocument();
		expect(screen.getByText("time:2026-01-02T03:04:05")).toBeInTheDocument();
		expect(screen.queryByText("image.bin")).not.toBeInTheDocument();

		fireEvent.change(
			screen.getByRole("searchbox", { name: "archive_preview_search" }),
			{ target: { value: "image" } },
		);

		expect(screen.getByText("image.bin")).toBeInTheDocument();
		expect(screen.queryByText("readme.txt")).not.toBeInTheDocument();
	});

	it("shows an extract compatibility notice for preview-only entry names", async () => {
		const loadManifest = vi.fn(async () => ({
			...manifest,
			extract_compatibility: {
				supported: false,
				reason: "unsupported_entry_names" as const,
			},
		}));
		render(<ArchivePreview loadManifest={loadManifest} />);

		expect(await screen.findByText("docs")).toBeInTheDocument();
		expect(
			screen.getByText("archive_preview_extract_unsupported_entry_names"),
		).toBeInTheDocument();
	});

	it("shows implicit folders as navigable entries", async () => {
		const loadManifest = vi.fn(async () => ({
			...manifest,
			entry_count: 1,
			directory_count: 1,
			entries: [
				{
					path: "src/main.rs",
					name: "main.rs",
					parent: "src",
					kind: "file",
					size: 9,
					compressed_size: 9,
					modified_at: null,
				},
			],
		}));
		render(<ArchivePreview loadManifest={loadManifest} />);

		expect(await screen.findByText("src")).toBeInTheDocument();
		expect(screen.queryByText("main.rs")).not.toBeInTheDocument();

		fireEvent.click(screen.getByText("src"));

		expect(screen.getByText("main.rs")).toBeInTheDocument();
	});

	it("opens directories with keyboard and navigates back with breadcrumbs", async () => {
		const loadManifest = vi.fn(async () => manifest);
		render(<ArchivePreview loadManifest={loadManifest} />);

		const docs = await screen.findByText("docs");
		fireEvent.keyDown(docs.closest("tr") as HTMLTableRowElement, {
			key: "Enter",
		});

		expect(screen.getByText("readme.txt")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "root" }));

		expect(screen.getByText("image.bin")).toBeInTheDocument();
		expect(screen.queryByText("readme.txt")).not.toBeInTheDocument();
	});

	it("shows empty and no-match states", async () => {
		const loadManifest = vi.fn(async () => ({
			...manifest,
			entry_count: 0,
			file_count: 0,
			directory_count: 0,
			total_uncompressed_size: 0,
			truncated: false,
			entries: [],
		}));
		render(<ArchivePreview loadManifest={loadManifest} />);

		expect(
			await screen.findByText("archive_preview_empty"),
		).toBeInTheDocument();

		fireEvent.change(
			screen.getByRole("searchbox", { name: "archive_preview_search" }),
			{ target: { value: "missing" } },
		);

		expect(screen.getByText("archive_preview_no_matches")).toBeInTheDocument();
	});

	it("counts visible items separately from raw archive entries", async () => {
		const loadManifest = vi.fn(async () => ({
			...manifest,
			entry_count: 146,
			file_count: 146,
			directory_count: 19,
		}));
		render(<ArchivePreview loadManifest={loadManifest} />);

		expect(await screen.findByText("docs")).toBeInTheDocument();
		expect(screen.getByText("archive_preview_items")).toBeInTheDocument();
		expect(screen.getByText("num:165")).toBeInTheDocument();
		expect(screen.getByText("archive_preview_entries")).toBeInTheDocument();
		const archiveEntrySummary = screen.getByText(
			"archive_preview_entries",
		).parentElement;
		expect(archiveEntrySummary).not.toBeNull();
		expect(
			within(archiveEntrySummary as HTMLElement).getByText("num:146"),
		).toBeInTheDocument();
	});

	it("hides filename encoding selector for formats that do not use it", async () => {
		const loadManifest = vi.fn(async () => sevenZipManifest);
		render(<ArchivePreview loadManifest={loadManifest} />);

		expect(await screen.findByText("docs")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", {
				name: "archive_preview_filename_encoding",
			}),
		).not.toBeInTheDocument();
	});

	it("shows retry UI when loading fails", async () => {
		const loadManifest = vi.fn(async () => {
			throw new Error("boom");
		});
		render(<ArchivePreview loadManifest={loadManifest} />);

		await waitFor(() => {
			expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		});
		expect(
			screen.getByRole("button", { name: "archive_preview_filename_encoding" }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "preview_retry" }),
		).toBeInTheDocument();
	});

	it("retries loading after a generic failure", async () => {
		const loadManifest = vi
			.fn<
				(options?: {
					signal?: AbortSignal;
					filenameEncoding?: "auto" | "utf8" | "gb18030" | "cp437";
				}) => Promise<ArchivePreviewManifest>
			>()
			.mockRejectedValueOnce(new Error("boom"))
			.mockResolvedValueOnce(manifest);
		render(<ArchivePreview loadManifest={loadManifest} />);

		fireEvent.click(
			await screen.findByRole("button", { name: "preview_retry" }),
		);

		expect(await screen.findByText("docs")).toBeInTheDocument();
		expect(loadManifest).toHaveBeenCalledTimes(2);
	});

	it("keeps loading and polls while archive preview is being generated", async () => {
		vi.useFakeTimers();
		const loadManifest = vi
			.fn<
				(options?: {
					signal?: AbortSignal;
					filenameEncoding?: "auto" | "utf8" | "gb18030" | "cp437";
				}) => Promise<ArchivePreviewManifest>
			>()
			.mockRejectedValueOnce(
				new ApiPendingError("Request is still processing", 1),
			)
			.mockResolvedValueOnce(manifest);

		try {
			render(<ArchivePreview loadManifest={loadManifest} />);

			await act(async () => {
				await Promise.resolve();
			});
			expect(
				screen.getByText("archive_preview_generating"),
			).toBeInTheDocument();
			expect(
				screen.getByRole("button", {
					name: "archive_preview_filename_encoding",
				}),
			).toBeInTheDocument();
			expect(screen.queryByText("preview_load_failed")).not.toBeInTheDocument();

			await act(async () => {
				vi.advanceTimersByTime(1000);
				await Promise.resolve();
				await Promise.resolve();
			});

			expect(screen.getByText("docs")).toBeInTheDocument();
			expect(loadManifest).toHaveBeenCalledTimes(2);
		} finally {
			vi.useRealTimers();
		}
	});

	it("reloads the manifest when filename encoding changes", async () => {
		const loadManifest = vi.fn(async () => manifest);
		render(<ArchivePreview loadManifest={loadManifest} />);

		expect(await screen.findByText("docs")).toBeInTheDocument();
		fireEvent.click(screen.getByText("select-gb18030"));

		expect(screen.getByText("archive_preview_generating")).toBeInTheDocument();
		expect(screen.queryByText("docs")).not.toBeInTheDocument();
		await waitFor(() => {
			expect(loadManifest).toHaveBeenCalledWith(
				expect.objectContaining({ filenameEncoding: "gb18030" }),
			);
		});
		expect(loadManifest).toHaveBeenCalledTimes(2);
	});

	it("keeps the archive shell mounted after an encoding failure so another encoding can be selected", async () => {
		const loadManifest = vi
			.fn<
				(options?: {
					signal?: AbortSignal;
					filenameEncoding?: "auto" | "utf8" | "gb18030" | "cp437";
				}) => Promise<ArchivePreviewManifest>
			>()
			.mockRejectedValueOnce(
				new ApiError(
					ErrorCode.BadRequest,
					"archive entry filename is not valid Big5",
					{
						internalCode: "E005",
						subcode: ApiSubcode.ArchivePreviewRejected,
					},
				),
			)
			.mockResolvedValueOnce(manifest);
		render(<ArchivePreview loadManifest={loadManifest} />);

		await waitFor(() => {
			expect(
				screen.getByText("archive_preview_encoding_failed"),
			).toBeInTheDocument();
		});
		expect(screen.getByRole("alert")).toHaveClass(
			"min-h-[14rem]",
			"items-center",
			"justify-center",
		);
		expect(
			screen.getByRole("button", { name: "archive_preview_filename_encoding" }),
		).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "preview_retry" }),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByText("select-gb18030"));

		expect(screen.getByText("archive_preview_generating")).toBeInTheDocument();
		expect(await screen.findByText("docs")).toBeInTheDocument();
		expect(loadManifest).toHaveBeenLastCalledWith(
			expect.objectContaining({ filenameEncoding: "gb18030" }),
		);
	});

	it("passes an abort signal to the loader and aborts it on unmount", async () => {
		let capturedSignal: AbortSignal | undefined;
		const loadManifest = vi.fn(
			({
				signal,
			}: {
				signal?: AbortSignal;
				filenameEncoding?: "auto" | "utf8" | "gb18030" | "cp437";
			} = {}) =>
				new Promise<ArchivePreviewManifest>(() => {
					capturedSignal = signal;
				}),
		);
		const { unmount } = render(<ArchivePreview loadManifest={loadManifest} />);

		await waitFor(() => {
			expect(capturedSignal).toBeInstanceOf(AbortSignal);
		});
		expect(capturedSignal?.aborted).toBe(false);

		unmount();

		expect(capturedSignal?.aborted).toBe(true);
	});

	it("ignores a loader result after unmount", async () => {
		let resolveManifest: (value: ArchivePreviewManifest) => void = () => {};
		const loadManifest = vi.fn(
			() =>
				new Promise<ArchivePreviewManifest>((resolve) => {
					resolveManifest = resolve;
				}),
		);
		const { unmount } = render(<ArchivePreview loadManifest={loadManifest} />);

		unmount();
		await act(async () => {
			resolveManifest(manifest);
		});

		expect(loadManifest).toHaveBeenCalledTimes(1);
	});

	it("shows a friendly disabled state without retry", async () => {
		const loadManifest = vi.fn(async () => {
			throw new ApiError(ErrorCode.Forbidden, "archive preview is disabled", {
				internalCode: "E013",
				subcode: ApiSubcode.ArchivePreviewDisabled,
			});
		});
		render(<ArchivePreview loadManifest={loadManifest} />);

		await waitFor(() => {
			expect(screen.getByText("archive_preview_disabled")).toBeInTheDocument();
		});
		expect(
			screen.getByRole("button", { name: "archive_preview_filename_encoding" }),
		).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "preview_retry" }),
		).not.toBeInTheDocument();
	});

	it.each([
		ApiSubcode.ArchivePreviewUserDisabled,
		ApiSubcode.ArchivePreviewShareDisabled,
	])("shows disabled state for %s", async (subcode) => {
		const loadManifest = vi.fn(async () => {
			throw new ApiError(
				ErrorCode.Forbidden,
				"archive preview for this surface is disabled",
				{
					internalCode: "E013",
					subcode,
				},
			);
		});
		render(<ArchivePreview loadManifest={loadManifest} />);

		await waitFor(() => {
			expect(screen.getByText("archive_preview_disabled")).toBeInTheDocument();
		});
		expect(
			screen.queryByRole("button", { name: "preview_retry" }),
		).not.toBeInTheDocument();
	});

	it.each([
		[
			ApiSubcode.ArchivePreviewUnsupportedType,
			"archive preview currently supports .zip and .7z files only",
			"archive_preview_unsupported_type",
		],
		[
			ApiSubcode.ArchivePreviewSourceTooLarge,
			"source archive size 135064658 exceeds archive preview limit 67108864",
			"archive_preview_source_too_large",
		],
		[
			ApiSubcode.ArchivePreviewInvalidArchive,
			"invalid archive",
			"archive_preview_invalid_archive",
		],
		[
			ApiSubcode.ArchivePreviewRejected,
			"archive contains 2 entries, exceeds server limit 1",
			"archive_preview_rejected",
		],
		[
			ApiSubcode.ArchivePreviewManifestTooLarge,
			"archive preview manifest exceeds server limit",
			"archive_preview_rejected",
		],
		[
			ApiSubcode.ArchivePreviewSourceSizeMismatch,
			"source archive size mismatch",
			"archive_preview_rejected",
		],
	])("shows a friendly state without retry for %s", async (subcode, message, messageKey) => {
		const loadManifest = vi.fn(async () => {
			throw new ApiError(ErrorCode.BadRequest, message, {
				internalCode: "E005",
				subcode,
			});
		});
		render(<ArchivePreview loadManifest={loadManifest} />);

		await waitFor(() => {
			expect(screen.getByText(messageKey)).toBeInTheDocument();
		});
		expect(
			screen.queryByRole("button", { name: "preview_retry" }),
		).not.toBeInTheDocument();
	});

	it("explains filename encoding failures without a retry button", async () => {
		const loadManifest = vi.fn(async () => {
			throw new ApiError(
				ErrorCode.BadRequest,
				"archive entry 'x.drawio' filename is not valid Big5",
				{
					internalCode: "E005",
					subcode: ApiSubcode.ArchivePreviewRejected,
				},
			);
		});
		render(<ArchivePreview loadManifest={loadManifest} />);

		await waitFor(() => {
			expect(
				screen.getByText("archive_preview_encoding_failed"),
			).toBeInTheDocument();
		});
		expect(
			screen.getByRole("button", { name: "archive_preview_filename_encoding" }),
		).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "preview_retry" }),
		).not.toBeInTheDocument();
	});
});

import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileInfoDialog } from "@/components/files/FileInfoDialog";

const mockState = vi.hoisted(() => ({
	getFile: vi.fn(),
	getFolderInfo: vi.fn(),
	getMediaMetadata: vi.fn(),
	listFolder: vi.fn(),
}));

function setDesktopMode(matches: boolean) {
	vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
		matches,
		media: query,
		onchange: null,
		addEventListener: vi.fn(),
		removeEventListener: vi.fn(),
		addListener: vi.fn(),
		removeListener: vi.fn(),
		dispatchEvent: vi.fn(),
	}));
}

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, opts?: Record<string, unknown>) => {
			if (key === "info_children_count") {
				return `folders:${opts?.folders} files:${opts?.files}`;
			}
			if (key === "info_loading") {
				return "loading";
			}
			if (key === "info_media_audio_tracks_count") {
				return `audio tracks:${opts?.count}`;
			}
			if (key === "info_media_subtitle_tracks_count") {
				return `subtitles:${opts?.count}`;
			}
			if (key === "info_media_orientation_landscape") {
				return "landscape";
			}
			if (key === "info_media_orientation_portrait") {
				return "portrait";
			}
			return key;
		},
	}),
}));

vi.mock("@/components/files/FileItemStatusIndicators", () => ({
	FileItemStatusIndicators: ({
		isLocked,
		isShared,
	}: {
		isLocked?: boolean;
		isShared?: boolean;
	}) => (
		<div>{`status:${isLocked ? "locked" : "unlocked"}:${isShared ? "shared" : "private"}`}</div>
	),
}));

vi.mock("@/components/files/FileTypeIcon", () => ({
	FileTypeIcon: ({ fileName }: { fileName?: string }) => (
		<div>{`type:${fileName ?? "unknown"}`}</div>
	),
}));

vi.mock("@/components/files/FileThumbnail", () => ({
	FileThumbnail: ({
		file,
		size,
	}: {
		file: { name: string };
		size?: string;
	}) => (
		<div data-testid="file-thumbnail">{`thumbnail:${file.name}:${size}`}</div>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div data-testid="dialog">{children}</div> : null,
	DialogContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogHeader: ({ children, className }: React.ComponentProps<"div">) => (
		<div className={className}>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({ children, className }: React.ComponentProps<"div">) => (
		<div className={className}>{children}</div>
	),
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
	formatDateAbsolute: (value: string) => `date:${value}`,
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		getFile: (...args: unknown[]) => mockState.getFile(...args),
		getFolderInfo: (...args: unknown[]) => mockState.getFolderInfo(...args),
		getMediaMetadata: (...args: unknown[]) =>
			mockState.getMediaMetadata(...args),
		listFolder: (...args: unknown[]) => mockState.listFolder(...args),
	},
}));

describe("FileInfoDialog", () => {
	beforeEach(() => {
		setDesktopMode(false);
		mockState.getFile.mockReset();
		mockState.getFolderInfo.mockReset();
		mockState.getMediaMetadata.mockReset();
		mockState.listFolder.mockReset();
	});

	it("renders file overview rows without requesting folder counts", () => {
		render(
			<FileInfoDialog
				open
				onOpenChange={vi.fn()}
				file={
					{
						blob_id: 88,
						created_at: "2026-01-01T00:00:00Z",
						id: 1,
						is_locked: true,
						mime_type: "text/markdown",
						name: "notes.md",
						size: 512,
						updated_at: "2026-01-02T00:00:00Z",
					} as never
				}
			/>,
		);

		expect(
			screen.getByRole("heading", { name: "notes.md" }),
		).toBeInTheDocument();
		expect(screen.getByTestId("file-thumbnail")).toHaveTextContent(
			"thumbnail:notes.md:lg",
		);
		expect(screen.getAllByText("bytes:512").length).toBe(2);
		expect(screen.getByText("text/markdown")).toBeInTheDocument();
		expect(screen.getByText("date:2026-01-01T00:00:00Z")).toBeInTheDocument();
		expect(screen.getByText("date:2026-01-02T00:00:00Z")).toBeInTheDocument();
		expect(screen.getByText("info_locked_yes")).toBeInTheDocument();
		expect(screen.getByText("status:locked:private")).toBeInTheDocument();
		expect(mockState.getFile).not.toHaveBeenCalled();
		expect(mockState.getFolderInfo).not.toHaveBeenCalled();
		expect(mockState.getMediaMetadata).not.toHaveBeenCalled();
		expect(mockState.listFolder).not.toHaveBeenCalled();
	});

	it("loads folder details and child counts when opened and shows the resolved totals", async () => {
		let resolveList:
			| ((value: { files_total: number; folders_total: number }) => void)
			| undefined;

		mockState.getFolderInfo.mockResolvedValueOnce({
			created_at: "2026-02-01T00:00:00Z",
			id: 3,
			is_locked: false,
			name: "Projects",
			policy_id: null,
			updated_at: "2026-02-02T00:00:00Z",
		});
		mockState.listFolder.mockImplementationOnce(
			() =>
				new Promise<{ files_total: number; folders_total: number }>(
					(resolve) => {
						resolveList = resolve;
					},
				),
		);

		render(
			<FileInfoDialog
				open
				onOpenChange={vi.fn()}
				folder={
					{
						id: 3,
						is_locked: false,
						name: "Projects",
						updated_at: "2026-02-02T00:00:00Z",
					} as never
				}
			/>,
		);

		expect(mockState.getFolderInfo).toHaveBeenCalledWith(3);
		expect(mockState.listFolder).toHaveBeenCalledWith(3, {
			file_limit: 0,
			folder_limit: 0,
		});
		expect(screen.getAllByText("loading").length).toBeGreaterThan(0);

		resolveList?.({ files_total: 5, folders_total: 2 });

		expect((await screen.findAllByText("folders:2 files:5")).length).toBe(2);
	});

	it("resets loaded folder counts on close and falls back to loading after a failed refresh", async () => {
		mockState.getFolderInfo
			.mockResolvedValueOnce({
				created_at: "2026-03-01T00:00:00Z",
				id: 9,
				is_locked: true,
				name: "Archive",
				policy_id: 12,
				updated_at: "2026-03-02T00:00:00Z",
			})
			.mockRejectedValueOnce(new Error("unavailable"));
		mockState.listFolder
			.mockResolvedValueOnce({ files_total: 1, folders_total: 4 })
			.mockRejectedValueOnce(new Error("unavailable"));

		const folder = {
			id: 9,
			is_locked: true,
			name: "Archive",
			updated_at: "2026-03-02T00:00:00Z",
		} as never;

		const { rerender } = render(
			<FileInfoDialog open onOpenChange={vi.fn()} folder={folder} />,
		);

		expect((await screen.findAllByText("folders:4 files:1")).length).toBe(2);

		rerender(
			<FileInfoDialog open={false} onOpenChange={vi.fn()} folder={folder} />,
		);
		expect(screen.queryByTestId("dialog")).not.toBeInTheDocument();

		rerender(<FileInfoDialog open onOpenChange={vi.fn()} folder={folder} />);

		await waitFor(() => {
			expect(mockState.getFolderInfo).toHaveBeenCalledTimes(2);
			expect(mockState.listFolder).toHaveBeenCalledTimes(2);
		});
		expect(screen.getAllByText("loading").length).toBeGreaterThan(0);
		expect(screen.queryByText("folders:4 files:1")).not.toBeInTheDocument();
	});

	it("loads file details when opened from a list item", async () => {
		mockState.getFile.mockResolvedValueOnce({
			blob_id: 88,
			created_at: "2026-01-01T00:00:00Z",
			id: 1,
			is_locked: true,
			mime_type: "text/markdown",
			name: "notes.md",
			size: 512,
			updated_at: "2026-01-02T00:00:00Z",
		});

		render(
			<FileInfoDialog
				open
				onOpenChange={vi.fn()}
				file={
					{
						id: 1,
						is_locked: true,
						is_shared: false,
						mime_type: "text/markdown",
						name: "notes.md",
						size: 512,
						updated_at: "2026-01-02T00:00:00Z",
					} as never
				}
			/>,
		);

		expect(mockState.getFile).toHaveBeenCalledWith(1);
		expect(
			await screen.findByText("date:2026-01-01T00:00:00Z"),
		).toBeInTheDocument();
	});

	it("shows image EXIF metadata in the details panel", async () => {
		mockState.getMediaMetadata.mockResolvedValueOnce({
			blob_hash: "hash",
			blob_id: 88,
			error: null,
			kind: "image",
			metadata: {
				artist: "Aaron Liu",
				camera_make: "SONY",
				camera_model: "ILCE-7CM2",
				exposure_bias_ev: 0,
				exposure_time_seconds: 0.005,
				f_number: 2.8,
				flash_fired: false,
				focal_length_mm: 28,
				format: "image/jpeg",
				height: 3265,
				iso: 100,
				kind: "image",
				lens_model: "E 28-200mm F2.8-5.6 A071",
				software: "ILCE-7CM2 v1.01",
				taken_at: "2024-04-01T05:44:11",
				width: 4898,
			},
			parser: "image",
			parser_version: "1",
			status: "ready",
			updated_at: "2026-01-01T00:00:00Z",
		});

		render(
			<FileInfoDialog
				open
				onOpenChange={vi.fn()}
				file={
					{
						blob_id: 88,
						created_at: "2026-01-01T00:00:00Z",
						file_category: "image",
						id: 1,
						is_locked: false,
						mime_type: "image/jpeg",
						name: "photo.jpg",
						size: 512,
						updated_at: "2026-01-02T00:00:00Z",
					} as never
				}
			/>,
		);

		await waitFor(() => {
			expect(mockState.getMediaMetadata).toHaveBeenCalledWith(1, {
				signal: expect.any(AbortSignal),
			});
		});
		expect(
			await screen.findByText("info_media_metadata_image"),
		).toBeInTheDocument();
		expect(screen.getByText("info_exif_aperture")).toBeInTheDocument();
		expect(screen.getByText("ƒ/2.8")).toBeInTheDocument();
		expect(screen.getByText("info_exif_exposure")).toBeInTheDocument();
		expect(screen.getByText("1/200 info_exif_seconds")).toBeInTheDocument();
		expect(screen.getByText("info_exif_camera")).toBeInTheDocument();
		expect(screen.getByText("SONY ILCE-7CM2")).toBeInTheDocument();
		expect(screen.getByText("info_exif_lens")).toBeInTheDocument();
		expect(
			screen.getByText("28mm (E 28-200mm F2.8-5.6 A071)"),
		).toBeInTheDocument();
		expect(screen.getByText("info_exif_taken_at")).toBeInTheDocument();
		expect(screen.getByText("2024/4/1 05:44:11")).toBeInTheDocument();
		expect(screen.getByText("info_exif_resolution")).toBeInTheDocument();
		expect(screen.getByText("16.0 MP · 4898 x 3265")).toBeInTheDocument();
		expect(screen.getByText("Aaron Liu")).toBeInTheDocument();
		expect(screen.getByText("ILCE-7CM2 v1.01")).toBeInTheDocument();
	});

	it("shows audio metadata in the details panel", async () => {
		mockState.getMediaMetadata.mockResolvedValueOnce({
			blob_hash: "hash",
			blob_id: 88,
			error: null,
			kind: "audio",
			metadata: {
				album: "Metamorph",
				album_artist: "The Score",
				artist: "The Score",
				artists: ["The Score"],
				audio_bitrate: 320,
				bit_depth: 16,
				channels: 2,
				date: "2024",
				disc_number: 1,
				disc_total: 1,
				duration_ms: 193_000,
				embedded_picture_mime_type: "image/jpeg",
				genre: "Alternative",
				has_embedded_picture: true,
				kind: "audio",
				overall_bitrate: 321,
				sample_rate: 44_100,
				title: "Real Life",
				track_number: 4,
				track_total: 12,
			},
			parser: "lofty",
			parser_version: "1",
			status: "ready",
			updated_at: "2026-01-01T00:00:00Z",
		});

		render(
			<FileInfoDialog
				open
				onOpenChange={vi.fn()}
				file={
					{
						blob_id: 88,
						created_at: "2026-01-01T00:00:00Z",
						file_category: "audio",
						id: 1,
						is_locked: false,
						mime_type: "audio/mpeg",
						name: "The Score - Real Life.mp3",
						size: 512,
						updated_at: "2026-01-02T00:00:00Z",
					} as never
				}
			/>,
		);

		await waitFor(() => {
			expect(mockState.getMediaMetadata).toHaveBeenCalledWith(1, {
				signal: expect.any(AbortSignal),
			});
		});
		expect(
			await screen.findByText("info_media_metadata_audio"),
		).toBeInTheDocument();
		expect(screen.getByText("info_media_title")).toBeInTheDocument();
		expect(screen.getByText("Real Life")).toBeInTheDocument();
		expect(screen.getByText("info_media_artist")).toBeInTheDocument();
		expect(screen.getAllByText("The Score").length).toBeGreaterThan(0);
		expect(screen.getByText("info_media_album")).toBeInTheDocument();
		expect(screen.getByText("Metamorph")).toBeInTheDocument();
		expect(screen.getByText("info_media_duration")).toBeInTheDocument();
		expect(screen.getByText("3:13")).toBeInTheDocument();
		expect(screen.getByText("44.1 kHz")).toBeInTheDocument();
		expect(screen.getByText("info_media_channels_stereo")).toBeInTheDocument();
		expect(screen.getByText("16-bit")).toBeInTheDocument();
		expect(screen.getByText("320 kbps")).toBeInTheDocument();
		expect(screen.getByText("4/12")).toBeInTheDocument();
		expect(screen.getByText("image/jpeg")).toBeInTheDocument();
	});

	it("shows video metadata in the details panel", async () => {
		mockState.getMediaMetadata.mockResolvedValueOnce({
			blob_hash: "hash",
			blob_id: 88,
			error: null,
			kind: "video",
			metadata: {
				audio_bitrate: 192_000,
				audio_channels: 2,
				audio_codec: "aac",
				audio_sample_rate: 48_000,
				audio_stream_count: 1,
				bit_depth: 10,
				codec: "h264",
				color_primaries: "bt2020",
				color_space: "bt2020nc",
				color_transfer: "smpte2084",
				container: "mov,mp4,m4a,3gp,3g2,mj2",
				creation_time: "2024-04-01T05:44:11.000000Z",
				display_height: 1920,
				display_width: 1080,
				duration_ms: 192_000,
				frame_rate: "30000/1001",
				height: 1080,
				hdr_format: "HDR10",
				kind: "video",
				overall_bitrate: 9_100_000,
				pixel_format: "yuv420p10le",
				rotation_degrees: 90,
				subtitle_stream_count: 2,
				video_bitrate: 8_400_000,
				width: 1920,
			},
			parser: "ffprobe",
			parser_version: "1",
			status: "ready",
			updated_at: "2026-01-01T00:00:00Z",
		});

		render(
			<FileInfoDialog
				open
				onOpenChange={vi.fn()}
				file={
					{
						blob_id: 88,
						created_at: "2026-01-01T00:00:00Z",
						file_category: "video",
						id: 1,
						is_locked: false,
						mime_type: "video/mp4",
						name: "clip.mp4",
						size: 512,
						updated_at: "2026-01-02T00:00:00Z",
					} as never
				}
			/>,
		);

		await waitFor(() => {
			expect(mockState.getMediaMetadata).toHaveBeenCalledWith(1, {
				signal: expect.any(AbortSignal),
			});
		});
		expect(
			await screen.findByText("info_media_metadata_video"),
		).toBeInTheDocument();
		expect(screen.getByText("info_media_duration")).toBeInTheDocument();
		expect(screen.getByText("3:12")).toBeInTheDocument();
		expect(screen.getByText("info_media_resolution")).toBeInTheDocument();
		expect(screen.getByText("1080 x 1920 · portrait")).toBeInTheDocument();
		expect(screen.getByText("info_media_codec")).toBeInTheDocument();
		expect(screen.getByText("H.264 / AVC")).toBeInTheDocument();
		expect(screen.getByText("info_media_frame_rate")).toBeInTheDocument();
		expect(screen.getByText("29.97 fps")).toBeInTheDocument();
		expect(screen.getByText("info_media_video_bitrate")).toBeInTheDocument();
		expect(screen.getByText("8.4 Mbps")).toBeInTheDocument();
		expect(screen.getByText("info_media_overall_bitrate")).toBeInTheDocument();
		expect(screen.getByText("9.1 Mbps")).toBeInTheDocument();
		expect(screen.getByText("info_media_color")).toBeInTheDocument();
		expect(
			screen.getByText("HDR10 · 10-bit · BT.2020 · PQ"),
		).toBeInTheDocument();
		expect(screen.getByText("info_media_audio")).toBeInTheDocument();
		expect(
			screen.getByText("AAC · info_media_channels_stereo · 48 kHz · 192 kbps"),
		).toBeInTheDocument();
		expect(screen.getByText("info_media_subtitles")).toBeInTheDocument();
		expect(screen.getByText("subtitles:2")).toBeInTheDocument();
		expect(screen.getByText("info_media_created_at")).toBeInTheDocument();
		expect(screen.getByText("2024/4/1 05:44:11")).toBeInTheDocument();
		expect(screen.getByText("info_media_container")).toBeInTheDocument();
		expect(screen.getByText("MP4 / QuickTime")).toBeInTheDocument();
	});

	it("renders a desktop inspector without quick actions and with close control", () => {
		setDesktopMode(true);
		const onOpenChange = vi.fn();
		const onPreview = vi.fn();
		const onShare = vi.fn();
		const onRename = vi.fn();

		render(
			<FileInfoDialog
				open
				onOpenChange={onOpenChange}
				onPreview={onPreview}
				onShare={onShare}
				onRename={onRename}
				file={
					{
						blob_id: 77,
						created_at: "2026-03-31T00:00:00Z",
						id: 7,
						is_locked: false,
						is_shared: true,
						mime_type: "application/pdf",
						name: "manual.pdf",
						size: 2048,
						updated_at: "2026-04-01T00:00:00Z",
					} as never
				}
			/>,
		);

		expect(screen.queryByTestId("dialog")).not.toBeInTheDocument();
		const inspector = screen.getByLabelText("info");
		expect(inspector).toBeInTheDocument();
		expect(inspector).toHaveClass("flex", "overflow-hidden");
		expect(inspector.firstElementChild).toHaveClass(
			"h-full",
			"min-h-0",
			"flex-1",
		);
		expect(screen.queryByRole("button", { name: "preview" })).toBeNull();
		expect(screen.queryByRole("button", { name: "share" })).toBeNull();
		expect(screen.queryByRole("button", { name: "rename" })).toBeNull();
		expect(onPreview).not.toHaveBeenCalled();
		expect(onShare).not.toHaveBeenCalled();
		expect(onRename).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("button", { name: "close" }));
		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it("does not expose lock toggling from the info panel", () => {
		setDesktopMode(true);
		const onToggleLock = vi.fn().mockResolvedValue(true);

		render(
			<FileInfoDialog
				open
				onOpenChange={vi.fn()}
				onToggleLock={onToggleLock}
				file={
					{
						blob_id: 77,
						created_at: "2026-03-31T00:00:00Z",
						id: 7,
						is_locked: false,
						is_shared: false,
						mime_type: "application/pdf",
						name: "manual.pdf",
						size: 2048,
						updated_at: "2026-04-01T00:00:00Z",
					} as never
				}
			/>,
		);

		expect(screen.queryByRole("button", { name: "lock" })).toBeNull();
		expect(onToggleLock).not.toHaveBeenCalled();
		expect(screen.getByText("info_locked_no")).toBeInTheDocument();
		expect(screen.getByText("status:unlocked:private")).toBeInTheDocument();
	});

	it("keeps the desktop inspector mounted long enough to animate out", () => {
		setDesktopMode(true);
		vi.useFakeTimers();

		const file = {
			blob_id: 88,
			created_at: "2026-01-01T00:00:00Z",
			id: 1,
			is_locked: true,
			mime_type: "text/markdown",
			name: "notes.md",
			size: 512,
			updated_at: "2026-01-02T00:00:00Z",
		} as never;

		const { rerender } = render(
			<FileInfoDialog open onOpenChange={vi.fn()} file={file} />,
		);

		rerender(
			<FileInfoDialog open={false} onOpenChange={vi.fn()} file={file} />,
		);
		expect(screen.getByLabelText("info")).toBeInTheDocument();

		act(() => {
			vi.advanceTimersByTime(320);
		});
		expect(screen.queryByLabelText("info")).not.toBeInTheDocument();
	});
});

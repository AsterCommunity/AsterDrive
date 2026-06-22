import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { VideoPreview } from "@/components/files/preview/VideoPreview";

const mockState = vi.hoisted(() => ({
	artplayerInstances: [] as Array<{
		options: {
			url: string;
			moreVideoAttr?: Record<string, unknown>;
		};
		destroy: ReturnType<typeof vi.fn>;
		template: { $video: HTMLVideoElement };
	}>,
	loggerWarn: vi.fn(),
	prepareAuthenticatedResource: vi.fn(),
	useBlobUrl: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		i18n: { language: "en" },
		t: (key: string) => key,
	}),
}));

vi.mock("@/hooks/useBlobUrl", () => ({
	useBlobUrl: (...args: unknown[]) => mockState.useBlobUrl(...args),
}));

vi.mock("@/lib/authenticatedResource", () => ({
	prepareAuthenticatedResource: (...args: unknown[]) =>
		mockState.prepareAuthenticatedResource(...args),
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.loggerWarn(...args),
	},
}));

vi.mock("artplayer", () => ({
	default: vi.fn().mockImplementation(function ArtplayerMock(options) {
		if (options.url.includes("throw-player")) {
			throw new Error("player init failed");
		}
		const instance = {
			options,
			destroy: vi.fn(),
			template: { $video: document.createElement("video") },
		};
		mockState.artplayerInstances.push(instance);
		return instance;
	}),
}));

describe("VideoPreview", () => {
	beforeEach(() => {
		mockState.artplayerInstances = [];
		mockState.loggerWarn.mockReset();
		mockState.prepareAuthenticatedResource.mockReset();
		mockState.prepareAuthenticatedResource.mockResolvedValue(undefined);
		mockState.useBlobUrl.mockReset();
		HTMLMediaElement.prototype.load = vi.fn();
	});

	it("renders loading while the video preview path is resolving", () => {
		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path={null}
			/>,
		);

		expect(screen.getByText("loading_preview")).toBeInTheDocument();
		expect(mockState.prepareAuthenticatedResource).not.toHaveBeenCalled();
		expect(mockState.artplayerInstances).toHaveLength(0);
	});

	it("prepares and passes the HTTP download URL to Artplayer", async () => {
		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/files/7/download"
			/>,
		);

		expect(mockState.useBlobUrl).not.toHaveBeenCalled();
		expect(mockState.prepareAuthenticatedResource).toHaveBeenCalledWith(
			"/files/7/download",
		);
		await waitFor(() => {
			expect(mockState.artplayerInstances).toHaveLength(1);
		});
		expect(mockState.artplayerInstances[0].options.url).toBe(
			"/api/v1/files/7/download",
		);
		expect(mockState.artplayerInstances[0].options.moreVideoAttr).toMatchObject(
			{
				preload: "metadata",
			},
		);
	});

	it("keeps already public preview URLs unchanged", async () => {
		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/pv/token/clip.mp4"
			/>,
		);

		await waitFor(() => expect(mockState.artplayerInstances).toHaveLength(1));
		expect(mockState.prepareAuthenticatedResource).toHaveBeenCalledWith(
			"/pv/token/clip.mp4",
		);
		expect(mockState.artplayerInstances[0].options.url).toBe(
			"/pv/token/clip.mp4",
		);
	});

	it("creates a stream session before initializing Artplayer when provided", async () => {
		const mediaStreamLinkFactory = vi.fn(async () => ({
			expires_at: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session-token/clip.mp4",
		}));

		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		expect(mockState.artplayerInstances).toHaveLength(0);
		await waitFor(() => {
			expect(mockState.artplayerInstances).toHaveLength(1);
		});
		expect(mediaStreamLinkFactory).toHaveBeenCalledTimes(1);
		expect(mockState.artplayerInstances[0].options.url).toBe(
			"/api/v1/s/share-token/stream/session-token/clip.mp4",
		);
	});

	it("renders loading while creating a stream session and an error when creation fails", async () => {
		const streamError = new Error("stream failed");
		const mediaStreamLinkFactory = vi.fn(async () => {
			throw streamError;
		});

		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		expect(screen.getByText("loading_preview")).toBeInTheDocument();
		await waitFor(() => {
			expect(screen.getByRole("alert")).toBeInTheDocument();
		});
		expect(mockState.loggerWarn).toHaveBeenCalledWith(
			"media stream session creation failed",
			"clip.mp4",
			streamError,
		);
	});

	it("falls back to a native video element when Artplayer initialization fails", async () => {
		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/throw-player"
			/>,
		);

		const nativeVideo = await screen.findByLabelText("clip.mp4");
		expect(nativeVideo).toHaveAttribute("src", "/api/v1/throw-player");
		expect(mockState.loggerWarn).toHaveBeenCalledWith(
			"artplayer init failed",
			"clip.mp4",
			expect.any(Error),
		);

		fireEvent.error(nativeVideo);
		expect(await screen.findByRole("alert")).toBeInTheDocument();
	});

	it("shows an error when the Artplayer-managed video element fails", async () => {
		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/files/7/download"
			/>,
		);

		await waitFor(() => {
			expect(mockState.artplayerInstances).toHaveLength(1);
		});
		const playerVideo = mockState.artplayerInstances[0].template.$video;
		fireEvent.error(playerVideo);

		expect(await screen.findByRole("alert")).toBeInTheDocument();
	});

	it("shows an error when protected media preparation fails with an auth error", async () => {
		const authError = { status: 401 };
		mockState.prepareAuthenticatedResource.mockRejectedValue(authError);

		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/files/7/download"
			/>,
		);

		expect(await screen.findByRole("alert")).toBeInTheDocument();
		expect(mockState.artplayerInstances).toHaveLength(0);
		expect(mockState.loggerWarn).toHaveBeenCalledWith(
			"media resource preparation failed",
			"clip.mp4",
			authError,
		);
	});
});

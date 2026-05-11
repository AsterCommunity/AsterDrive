import { render, waitFor } from "@testing-library/react";
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

vi.mock("artplayer", () => ({
	default: vi.fn().mockImplementation(function ArtplayerMock(options) {
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
		mockState.useBlobUrl.mockReset();
		HTMLMediaElement.prototype.load = vi.fn();
	});

	it("passes the HTTP download URL directly to Artplayer", () => {
		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/files/7/download"
			/>,
		);

		expect(mockState.useBlobUrl).not.toHaveBeenCalled();
		expect(mockState.artplayerInstances).toHaveLength(1);
		expect(mockState.artplayerInstances[0].options.url).toBe(
			"/api/v1/files/7/download",
		);
		expect(mockState.artplayerInstances[0].options.moreVideoAttr).toMatchObject(
			{
				preload: "metadata",
			},
		);
	});

	it("keeps already public preview URLs unchanged", () => {
		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/pv/token/clip.mp4"
			/>,
		);

		expect(mockState.artplayerInstances[0].options.url).toBe(
			"/pv/token/clip.mp4",
		);
	});

	it("creates a stream session before initializing Artplayer when provided", async () => {
		const videoStreamLinkFactory = vi.fn(async () => ({
			expires_at: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session-token/clip.mp4",
		}));

		render(
			<VideoPreview
				file={{ name: "clip.mp4", mime_type: "video/mp4" }}
				path="/s/share-token/download"
				videoStreamLinkFactory={videoStreamLinkFactory}
			/>,
		);

		expect(mockState.artplayerInstances).toHaveLength(0);
		await waitFor(() => {
			expect(mockState.artplayerInstances).toHaveLength(1);
		});
		expect(videoStreamLinkFactory).toHaveBeenCalledTimes(1);
		expect(mockState.artplayerInstances[0].options.url).toBe(
			"/api/v1/s/share-token/stream/session-token/clip.mp4",
		);
	});
});

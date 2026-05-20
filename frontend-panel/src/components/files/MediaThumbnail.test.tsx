import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MediaThumbnail } from "@/components/files/MediaThumbnail";

vi.mock("@/components/files/FileThumbnail", () => ({
	FileThumbnail: ({
		className,
		file,
		iconClassName,
		imageClassName,
		size,
		thumbnailPath,
	}: {
		className?: string;
		file: {
			id: number;
			mime_type: string;
			name: string;
		};
		iconClassName?: string;
		imageClassName?: string;
		size?: string;
		thumbnailPath?: string;
	}) => (
		<div
			className={className}
			data-file-id={file.id}
			data-file-mime={file.mime_type}
			data-file-name={file.name}
			data-icon-class={iconClassName ?? ""}
			data-image-class={imageClassName ?? ""}
			data-size={size ?? ""}
			data-testid="file-thumbnail"
			data-thumbnail-path={thumbnailPath ?? ""}
		/>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ className, name }: { className?: string; name: string }) => (
		<span
			className={className}
			data-icon-name={name}
			data-testid="media-icon"
		/>
	),
}));

const audioFile = {
	file_category: "audio" as const,
	id: 7,
	mime_type: "audio/mpeg",
	name: "track.mp3",
};

describe("MediaThumbnail", () => {
	it("delegates file thumbnails and fallback handling to FileThumbnail", () => {
		render(
			<MediaThumbnail
				file={audioFile}
				size="lg"
				thumbnailPath="/files/7/thumbnail"
				className="custom-shell"
				iconClassName="custom-icon"
				imageClassName="custom-image"
				artworkUrl="blob:artwork"
			/>,
		);

		const thumbnail = screen.getByTestId("file-thumbnail");
		expect(thumbnail).toHaveAttribute("data-file-id", "7");
		expect(thumbnail).toHaveAttribute("data-file-name", "track.mp3");
		expect(thumbnail).toHaveAttribute("data-file-mime", "audio/mpeg");
		expect(thumbnail).toHaveAttribute("data-size", "lg");
		expect(thumbnail).toHaveAttribute(
			"data-thumbnail-path",
			"/files/7/thumbnail",
		);
		expect(thumbnail).toHaveAttribute("data-icon-class", "custom-icon");
		expect(thumbnail).toHaveAttribute("data-image-class", "custom-image");
		expect(thumbnail).toHaveClass("custom-shell");
		expect(screen.queryByRole("img")).not.toBeInTheDocument();
	});

	it("renders parsed artwork when no thumbnail file is available", () => {
		const { container } = render(
			<MediaThumbnail
				artworkUrl="blob:parsed-artwork"
				className="art-shell"
				imageClassName="art-image"
			/>,
		);

		const image = container.querySelector("img");
		expect(image).toHaveAttribute("src", "blob:parsed-artwork");
		expect(image).toHaveAttribute("alt", "");
		expect(image).toHaveClass("art-shell");
		expect(image).toHaveClass("art-image");
		expect(screen.queryByTestId("file-thumbnail")).not.toBeInTheDocument();
	});

	it("falls back to the default media icon without a thumbnail file or artwork", () => {
		render(
			<MediaThumbnail
				className="fallback-shell"
				iconClassName="fallback-icon"
			/>,
		);

		const icon = screen.getByTestId("media-icon");
		expect(icon).toHaveAttribute("data-icon-name", "VinylRecord");
		expect(icon).toHaveClass("fallback-icon");
		expect(icon.parentElement).toHaveClass("fallback-shell");
		expect(screen.queryByTestId("file-thumbnail")).not.toBeInTheDocument();
		expect(screen.queryByRole("img")).not.toBeInTheDocument();
	});
});

import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UrlTemplatePreview } from "@/components/files/preview/viewers/external/UrlTemplatePreview";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import { deferred } from "@/test/deferred";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (options?.label) {
				return `${key}:${options.label}`;
			}
			return key;
		},
	}),
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({
		title,
		description,
		action,
	}: {
		action?: React.ReactNode;
		description: string;
		title: string;
	}) => (
		<div>
			<div>{title}</div>
			<div>{description}</div>
			{action}
		</div>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
	}: {
		children: React.ReactNode;
		onClick?: () => void;
	}) => (
		<button type="button" onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/files/preview/shared/PreviewLoadingState", () => ({
	PreviewLoadingState: ({ text }: { text: string }) => <div>{text}</div>,
}));

describe("UrlTemplatePreview", () => {
	const file = {
		id: 7,
		name: "clip.mp4",
		mime_type: "video/mp4",
		size: 2048,
	};

	it("shows an unavailable state when the target cannot be resolved", async () => {
		render(
			<UrlTemplatePreview
				file={file}
				downloadPath="/files/7/download"
				label="Viewer"
				rawConfig={{
					allowed_origins: [],
					mode: "iframe",
					url_template: "javascript:alert(1)",
				}}
			/>,
		);

		expect(
			await screen.findByText("url_template_unavailable"),
		).toBeInTheDocument();
		expect(
			screen.getByText("url_template_unavailable_desc"),
		).toBeInTheDocument();
	});

	it("shows an external open action for new-tab targets", async () => {
		const openSpy = vi.spyOn(window, "open").mockReturnValue(null);

		render(
			<UrlTemplatePreview
				file={file}
				downloadPath="/files/7/download"
				label="Jellyfin"
				rawConfig={{
					allowed_origins: ["https://videos.example.com"],
					mode: "new_tab",
					url_template: "https://videos.example.com/watch?id={{file_id}}",
				}}
			/>,
		);

		expect(await screen.findByText("Jellyfin")).toBeInTheDocument();
		expect(
			screen.getByText("url_template_external_desc:Jellyfin"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /url_template_open/i }));

		expect(openSpy).toHaveBeenCalledWith(
			"https://videos.example.com/watch?id=7",
			"_blank",
			"noopener,noreferrer",
		);
	});

	it("renders iframe targets inside the embedded web app shell", async () => {
		render(
			<UrlTemplatePreview
				file={file}
				downloadPath="/files/7/download"
				label="Viewer"
				rawConfig={{
					allowed_origins: ["https://viewer.example.com"],
					mode: "iframe",
					url_template: "https://viewer.example.com/open?id={{file_id}}",
				}}
			/>,
		);

		const iframe = await screen.findByTitle("Viewer");
		expect(iframe).toHaveAttribute(
			"src",
			"https://viewer.example.com/open?id=7",
		);
		expect(iframe).toHaveAttribute(
			"sandbox",
			"allow-scripts allow-forms allow-popups allow-downloads",
		);
		expect(iframe).toHaveAttribute(
			"allow",
			"autoplay; fullscreen; picture-in-picture",
		);
		expect(iframe).toHaveAttribute("referrerpolicy", "same-origin");
		expect(
			screen.getByRole("button", { name: /url_template_open/i }),
		).toBeInTheDocument();
	});

	it("uses trusted document viewer iframe permissions for known office URL template providers", async () => {
		render(
			<UrlTemplatePreview
				file={file}
				downloadPath="/files/7/download"
				label="Microsoft Viewer"
				optionKey="builtin.office_microsoft"
				rawConfig={{
					allowed_origins: ["https://view.officeapps.live.com"],
					mode: "iframe",
					url_template: "https://view.officeapps.live.com/open?id={{file_id}}",
				}}
			/>,
		);

		expect(await screen.findByTitle("Microsoft Viewer")).toHaveAttribute(
			"sandbox",
			"allow-scripts allow-forms allow-popups allow-downloads allow-same-origin allow-top-navigation allow-popups-to-escape-sandbox",
		);
		expect(screen.getByTitle("Microsoft Viewer")).toHaveAttribute(
			"allow",
			"autoplay; fullscreen; picture-in-picture; clipboard-read 'src'; clipboard-write 'src'",
		);
	});
	it("does not reveal a URL resolved after the preview starts exiting", async () => {
		const pending = deferred<{ url: string }>();
		const createLink = vi.fn(() => pending.promise);
		const view = (open: boolean) => (
			<DialogLifecycle open={open} target={null}>
				{() => (
					<UrlTemplatePreview
						file={file}
						downloadPath="/files/7/download"
						label="Viewer"
						rawConfig={{
							mode: "iframe",
							allowed_origins: ["https://office.example"],
							url_template: "https://office.example/?src={{file_preview_url}}",
						}}
						createExternalPreviewLink={createLink}
					/>
				)}
			</DialogLifecycle>
		);
		const { rerender } = render(view(true));
		expect(createLink).toHaveBeenCalledTimes(1);
		rerender(view(false));
		await act(async () => {
			pending.resolve({ url: "https://drive.example/late" });
			await pending.promise;
		});
		expect(screen.queryByTitle("Viewer")).toBeNull();
	});
});

import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MediaProcessingConfigEditor } from "@/components/admin/MediaProcessingConfigEditor";

const mockStatus = vi.hoisted(() => vi.fn());

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/services/adminService", () => ({
	adminConfigService: {
		mediaProcessingStatus: mockStatus,
	},
}));

const config = JSON.stringify({
	version: 2,
	processors: [
		{
			kind: "vips_cli",
			enabled: true,
			extensions: ["heic"],
			uses: ["thumbnail:image"],
			config: { command: "vips" },
		},
	],
});

describe("MediaProcessingConfigEditor", () => {
	beforeEach(() => {
		mockStatus.mockReset();
	});

	it("explains a saved enabled processor that is missing from the runtime", async () => {
		mockStatus.mockResolvedValue({
			version: 1,
			processors: [
				{
					kind: "vips_cli",
					configured_enabled: true,
					runtime_available: false,
					effective_enabled: false,
					unavailable_reason: "command_not_found",
				},
			],
		});

		render(<MediaProcessingConfigEditor value={config} onChange={vi.fn()} />);

		await waitFor(() => {
			expect(
				screen.getByText(
					"media_processing_editor_processor_runtime_unavailable",
				),
			).toBeInTheDocument();
		});
		expect(
			screen.getByText(
				"media_processing_editor_processor_runtime_unavailable_hint",
			),
		).toBeInTheDocument();
		expect(mockStatus).toHaveBeenCalledTimes(1);
	});

	it("shows runtime availability without an unavailable hint", async () => {
		mockStatus.mockResolvedValue({
			version: 1,
			processors: [
				{
					kind: "vips_cli",
					configured_enabled: true,
					runtime_available: true,
					effective_enabled: true,
				},
			],
		});

		render(<MediaProcessingConfigEditor value={config} onChange={vi.fn()} />);

		await waitFor(() => {
			expect(
				screen.getByText("media_processing_editor_processor_runtime_available"),
			).toBeInTheDocument();
		});
		expect(
			screen.queryByText(
				"media_processing_editor_processor_runtime_unavailable_hint",
			),
		).not.toBeInTheDocument();
	});

	it("keeps the editor usable when runtime status probing fails", async () => {
		mockStatus.mockRejectedValue(new Error("probe failed"));

		render(<MediaProcessingConfigEditor value={config} onChange={vi.fn()} />);

		expect(
			await screen.findByText("media_processing_editor_title"),
		).toBeInTheDocument();
	});

	it("ignores a runtime status result after the editor unmounts", async () => {
		let resolveStatus!: (value: unknown) => void;
		mockStatus.mockReturnValue(
			new Promise((resolve) => {
				resolveStatus = resolve;
			}),
		);

		const view = render(
			<MediaProcessingConfigEditor value={config} onChange={vi.fn()} />,
		);
		view.unmount();
		resolveStatus({ version: 1, processors: [] });
		await Promise.resolve();

		expect(mockStatus).toHaveBeenCalledTimes(1);
	});
});

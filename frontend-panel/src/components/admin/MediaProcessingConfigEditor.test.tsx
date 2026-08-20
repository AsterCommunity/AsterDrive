import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
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
});

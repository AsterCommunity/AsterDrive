import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { emptyForm } from "@/components/admin/storagePolicyDialogShared";
import { S3DownloadStrategyField } from "./S3DownloadStrategyField";
import { S3UploadStrategyField } from "./S3UploadStrategyField";
import type { Translate } from "./StoragePolicyFieldTypes";

const labels: Record<string, string> = {
	download_strategy_presigned: "Presigned download",
	download_strategy_presigned_desc: "Download directly from S3.",
	download_strategy_relay_stream: "Relay download",
	download_strategy_relay_stream_desc: "Download through the server.",
	s3_download_strategy: "S3 download strategy",
	s3_upload_strategy: "S3 upload strategy",
	upload_strategy_presigned: "Presigned upload",
	upload_strategy_presigned_desc: "Upload directly to S3.",
	upload_strategy_relay_stream: "Relay upload",
	upload_strategy_relay_stream_desc: "Upload through the server.",
};

const t: Translate = (key) => labels[key] ?? key;

describe("S3 strategy fields", () => {
	it("renders upload strategy copy for the selected mode", () => {
		render(
			<S3UploadStrategyField
				form={{ ...emptyForm, s3_upload_strategy: "presigned" }}
				onFieldChange={vi.fn()}
				t={t}
			/>,
		);

		expect(screen.getByText("S3 upload strategy")).toBeInTheDocument();
		expect(screen.getByText("Upload directly to S3.")).toBeInTheDocument();
	});

	it("renders download strategy copy for the selected mode", () => {
		render(
			<S3DownloadStrategyField
				form={{ ...emptyForm, s3_download_strategy: "relay_stream" }}
				onFieldChange={vi.fn()}
				t={t}
			/>,
		);

		expect(screen.getByText("S3 download strategy")).toBeInTheDocument();
		expect(
			screen.getByText("Download through the server."),
		).toBeInTheDocument();
	});
});

import { fireEvent, render, screen } from "@testing-library/react";
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

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
	}: {
		children: React.ReactNode;
		htmlFor?: string;
	}) => <label htmlFor={htmlFor}>{children}</label>,
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		items,
		onValueChange,
		value,
	}: {
		children: React.ReactNode;
		items?: Array<{ label: string; value: string }>;
		onValueChange?: (value: string) => void;
		value?: string;
	}) => (
		<div>
			<div>{`select:${value}`}</div>
			{items?.map((item) => (
				<button
					key={item.value}
					type="button"
					onClick={() => onValueChange?.(item.value)}
				>
					{`choose:${item.value}`}
				</button>
			))}
			{children}
		</div>
	),
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectTrigger: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectValue: () => <span>select-value</span>,
}));

describe("S3 strategy fields", () => {
	it("renders upload strategy copy for the selected mode", () => {
		const onFieldChange = vi.fn();
		render(
			<S3UploadStrategyField
				form={{ ...emptyForm, s3_upload_strategy: "presigned" }}
				onFieldChange={onFieldChange}
				t={t}
			/>,
		);

		expect(screen.getByText("S3 upload strategy")).toBeInTheDocument();
		expect(screen.getByText("Upload directly to S3.")).toBeInTheDocument();
		fireEvent.click(screen.getByText("choose:relay_stream"));
		expect(onFieldChange).toHaveBeenCalledWith(
			"s3_upload_strategy",
			"relay_stream",
		);
	});

	it("renders download strategy copy for the selected mode", () => {
		const onFieldChange = vi.fn();
		render(
			<S3DownloadStrategyField
				form={{ ...emptyForm, s3_download_strategy: "relay_stream" }}
				onFieldChange={onFieldChange}
				t={t}
			/>,
		);

		expect(screen.getByText("S3 download strategy")).toBeInTheDocument();
		expect(
			screen.getByText("Download through the server."),
		).toBeInTheDocument();
		fireEvent.click(screen.getByText("choose:presigned"));
		expect(onFieldChange).toHaveBeenCalledWith(
			"s3_download_strategy",
			"presigned",
		);
	});
});

import type { S3DownloadStrategy } from "@/components/admin/storagePolicyDialogShared";
import type { SelectOption, SharedFieldProps } from "./StoragePolicyFieldTypes";
import { StrategySelectField } from "./StoragePolicyStrategyFields";

export function S3DownloadStrategyField({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	const options = [
		{
			label: t("download_strategy_relay_stream"),
			value: "relay_stream",
		},
		{
			label: t("download_strategy_presigned"),
			value: "presigned",
		},
	] satisfies ReadonlyArray<SelectOption<S3DownloadStrategy>>;

	return (
		<StrategySelectField
			id="s3_download_strategy"
			label={t("s3_download_strategy")}
			options={options}
			value={form.s3_download_strategy}
			onChange={(value) => onFieldChange("s3_download_strategy", value)}
			description={t(
				form.s3_download_strategy === "relay_stream"
					? "download_strategy_relay_stream_desc"
					: "download_strategy_presigned_desc",
			)}
		/>
	);
}

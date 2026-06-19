import type { S3UploadStrategy } from "@/types/api";
import type { SelectOption, SharedFieldProps } from "./StoragePolicyFieldTypes";
import { StrategySelectField } from "./StoragePolicyStrategyFields";

export function S3UploadStrategyField({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	const options = [
		{
			label: t("upload_strategy_relay_stream"),
			value: "relay_stream",
		},
		{
			label: t("upload_strategy_presigned"),
			value: "presigned",
		},
	] satisfies ReadonlyArray<SelectOption<S3UploadStrategy>>;

	return (
		<StrategySelectField
			id="s3_upload_strategy"
			label={t("s3_upload_strategy")}
			options={options}
			value={form.s3_upload_strategy}
			onChange={(value) => onFieldChange("s3_upload_strategy", value)}
			description={t(
				form.s3_upload_strategy === "relay_stream"
					? "upload_strategy_relay_stream_desc"
					: "upload_strategy_presigned_desc",
			)}
		/>
	);
}

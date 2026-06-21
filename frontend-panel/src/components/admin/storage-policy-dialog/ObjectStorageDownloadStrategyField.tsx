import type { ObjectStorageDownloadStrategy } from "@/types/api";
import type { SelectOption, SharedFieldProps } from "./StoragePolicyFieldTypes";
import { StrategySelectField } from "./StoragePolicyStrategyFields";

export function ObjectStorageDownloadStrategyField({
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
	] satisfies ReadonlyArray<SelectOption<ObjectStorageDownloadStrategy>>;

	return (
		<StrategySelectField
			id="object_storage_download_strategy"
			label={t("object_storage_download_strategy")}
			options={options}
			value={form.object_storage_download_strategy}
			onChange={(value) =>
				onFieldChange("object_storage_download_strategy", value)
			}
			description={t(
				form.object_storage_download_strategy === "relay_stream"
					? "download_strategy_relay_stream_desc"
					: "download_strategy_presigned_desc",
			)}
		/>
	);
}

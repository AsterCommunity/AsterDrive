import type { ObjectStorageUploadStrategy } from "@/types/api";
import type { SelectOption, SharedFieldProps } from "./StoragePolicyFieldTypes";
import { StrategySelectField } from "./StoragePolicyStrategyFields";

export function ObjectStorageUploadStrategyField({
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
	] satisfies ReadonlyArray<SelectOption<ObjectStorageUploadStrategy>>;

	return (
		<StrategySelectField
			id="object_storage_upload_strategy"
			label={t("object_storage_upload_strategy")}
			options={options}
			value={form.object_storage_upload_strategy}
			onChange={(value) =>
				onFieldChange("object_storage_upload_strategy", value)
			}
			description={t(
				form.object_storage_upload_strategy === "relay_stream"
					? "upload_strategy_relay_stream_desc"
					: "upload_strategy_presigned_desc",
			)}
		/>
	);
}

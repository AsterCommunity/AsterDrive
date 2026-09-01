import { toast } from "sonner";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import type { StorageConnectorDescriptor } from "@/types/api";
import { missingRequiredConnectorFields } from "./connectorFieldRules";
import type { PolicyFormData } from "./formTypes";

type Translate = (
	key: string,
	values?: Record<string, number | string>,
) => string;

/**
 * 缺失必填字段时弹出 toast 列出字段名并返回 true，供调用方中止流程。
 * 编辑模式传 allowSavedCredentials：secret/credential 留空表示沿用已保存凭证。
 */
export function toastMissingRequiredConnectorFields(
	t: Translate,
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor | null | undefined,
	{ allowSavedCredentials = false }: { allowSavedCredentials?: boolean } = {},
): boolean {
	if (!descriptor) {
		return false;
	}
	const missing = missingRequiredConnectorFields(form, descriptor, {
		allowSavedCredentials,
	});
	if (missing.length === 0) {
		return false;
	}
	toast.error(
		t("policy_required_fields_missing", {
			fields: missing
				.map((field) =>
					String(
						translateStorageConnectorMessage(
							t,
							descriptor.connector_id,
							field.label_key,
						),
					),
				)
				.join(t("policy_field_list_separator")),
		}),
	);
	return true;
}

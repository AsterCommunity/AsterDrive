import { normalizeObjectStorageConnectionFields } from "@/lib/objectStorageConnectionFields";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
} from "@/types/api";

export type RemoteStorageTargetDriverType = string;

export function isRemoteStorageTargetDriverType(
	driverType: unknown,
): driverType is RemoteStorageTargetDriverType {
	return typeof driverType === "string" && driverType.trim().length > 0;
}

export interface RemoteStorageTargetFormData {
	name: string;
	driver_type: RemoteStorageTargetDriverType;
	endpoint: string;
	bucket: string;
	access_key: string;
	secret_key: string;
	base_path: string;
	is_default: boolean;
}

export type RemoteStorageTargetSupportedFields =
	| ReadonlySet<string>
	| Pick<RemoteStorageTargetDriverDescriptor, "fields">;

function toRemoteStorageTargetFieldSet(
	supportedFields: RemoteStorageTargetSupportedFields,
): ReadonlySet<string> {
	return "fields" in supportedFields
		? new Set(supportedFields.fields.map((field) => field.name))
		: supportedFields;
}

function supportedFieldValue(
	form: RemoteStorageTargetFormData,
	fieldNames: ReadonlySet<string>,
	fieldName: "access_key" | "bucket" | "endpoint" | "secret_key",
): string {
	return fieldNames.has(fieldName) ? form[fieldName].trim() : "";
}

export function getRemoteStorageTargetForm(
	profile: RemoteStorageTargetInfo,
): RemoteStorageTargetFormData {
	const legacy = profile as unknown as {
		driver_type?: string;
		endpoint?: string;
		bucket?: string;
		base_path?: string;
	};
	return {
		name: profile.name,
		driver_type: profile.connector_id ?? legacy.driver_type ?? "asterdrive.storage.local",
		endpoint: String(profile.connector_config?.values?.endpoint ?? legacy.endpoint ?? ""),
		bucket: String(profile.connector_config?.values?.bucket ?? legacy.bucket ?? ""),
		access_key: "",
		secret_key: "",
		base_path: String(profile.connector_config?.values?.base_path ?? legacy.base_path ?? "."),
		is_default: profile.is_default,
	};
}

function normalizeRemoteStorageTargetForm(
	form: RemoteStorageTargetFormData,
	supportedFields: RemoteStorageTargetSupportedFields,
): RemoteStorageTargetFormData {
	const fieldNames = toRemoteStorageTargetFieldSet(supportedFields);
	const endpoint = supportedFieldValue(form, fieldNames, "endpoint");
	const bucket = supportedFieldValue(form, fieldNames, "bucket");
	const shouldNormalizeObjectStorageFields =
		fieldNames.has("endpoint") && fieldNames.has("bucket");

	const normalized = shouldNormalizeObjectStorageFields
		? normalizeObjectStorageConnectionFields(endpoint, bucket)
		: { endpoint, bucket };
	return {
		...form,
		name: form.name.trim(),
		endpoint: normalized.endpoint,
		bucket: normalized.bucket,
		access_key: supportedFieldValue(form, fieldNames, "access_key"),
		secret_key: supportedFieldValue(form, fieldNames, "secret_key"),
		base_path: form.base_path.trim(),
	};
}

function connectorIdForDriverType(driverType: RemoteStorageTargetDriverType): string {
	if (driverType.startsWith("asterdrive.")) {
		return driverType;
	}
	const suffix =
		driverType === "azureblob"
			? "azure_blob"
			: driverType === "tencentcos"
				? "tencent_cos"
				: driverType === "alibabaoss"
					? "alibaba_oss"
					: driverType;
	return `asterdrive.storage.${suffix}`;
}

export function buildCreateRemoteStorageTargetPayload(
	form: RemoteStorageTargetFormData,
	supportedFields: RemoteStorageTargetSupportedFields,
): RemoteCreateStorageTargetRequest {
	const normalized = normalizeRemoteStorageTargetForm(form, supportedFields);

	return {
		name: normalized.name,
		driver_type: "connector",
		connector_config: {
			format_version: 1,
			connector_id: normalized.driver_type.startsWith("asterdrive.") ? normalized.driver_type : connectorIdForDriverType(normalized.driver_type),
			schema_version: 1,
			values: {
				endpoint: normalized.endpoint,
				bucket: normalized.bucket,
				base_path: normalized.base_path,
			},
		},
		credential: {
			access_key: normalized.access_key,
			secret_key: normalized.secret_key,
		},
		is_default: normalized.is_default,
	} as RemoteCreateStorageTargetRequest;
}

export function buildUpdateRemoteStorageTargetPayload(
	form: RemoteStorageTargetFormData,
	supportedFields: RemoteStorageTargetSupportedFields,
	editingTarget: RemoteStorageTargetInfo,
): RemoteUpdateStorageTargetRequest {
	const fieldNames = toRemoteStorageTargetFieldSet(supportedFields);
	const normalized = normalizeRemoteStorageTargetForm(form, fieldNames);
	const supportsAccessKey = fieldNames.has("access_key");
	const supportsSecretKey = fieldNames.has("secret_key");
	const legacyEditing = editingTarget as unknown as { driver_type?: string };
	const sameDriverType =
		(editingTarget.connector_id ??
			(legacyEditing.driver_type
				? connectorIdForDriverType(legacyEditing.driver_type)
				: "")) === connectorIdForDriverType(normalized.driver_type);
	const payload = {
		name: normalized.name,
		connector_config: {
			format_version: 1,
			connector_id: normalized.driver_type.startsWith("asterdrive.") ? normalized.driver_type : connectorIdForDriverType(normalized.driver_type),
			schema_version: 1,
			values: {
				endpoint: normalized.endpoint,
				bucket: normalized.bucket,
				base_path: normalized.base_path,
			},
		},
		is_default: normalized.is_default,
	} as unknown as RemoteUpdateStorageTargetRequest & { driver_type?: string };

	if (!supportsAccessKey && !supportsSecretKey) {
		return payload;
	}

	const accessKey = normalized.access_key;
	const secretKey = normalized.secret_key;
	if (supportsAccessKey && (!sameDriverType || accessKey)) {
		payload.credential = { access_key: accessKey };
	}
	if (supportsSecretKey && (!sameDriverType || secretKey)) {
		payload.credential = { ...(payload.credential ?? {}), secret_key: secretKey };
	}

	return payload;
}

export const emptyRemoteStorageTargetForm: RemoteStorageTargetFormData = {
	name: "",
	driver_type: "asterdrive.storage.local",
	endpoint: "",
	bucket: "",
	access_key: "",
	secret_key: "",
	base_path: ".",
	is_default: false,
};

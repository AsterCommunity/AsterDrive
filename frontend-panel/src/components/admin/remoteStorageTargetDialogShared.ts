import { normalizeObjectStorageConnectionFields } from "@/lib/objectStorageConnectionFields";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
} from "@/types/api";

export type RemoteStorageTargetDriverType = "local" | "s3";

export function isRemoteStorageTargetDriverType(
	driverType: unknown,
): driverType is RemoteStorageTargetDriverType {
	return driverType === "local" || driverType === "s3";
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
	return {
		name: profile.name,
		driver_type: profile.driver_type === "s3" ? "s3" : "local",
		endpoint: profile.endpoint,
		bucket: profile.bucket,
		access_key: "",
		secret_key: "",
		base_path: profile.base_path,
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

export function buildCreateRemoteStorageTargetPayload(
	form: RemoteStorageTargetFormData,
	supportedFields: RemoteStorageTargetSupportedFields,
): RemoteCreateStorageTargetRequest {
	const normalized = normalizeRemoteStorageTargetForm(form, supportedFields);

	return {
		name: normalized.name,
		driver_type: normalized.driver_type,
		endpoint: normalized.endpoint,
		bucket: normalized.bucket,
		access_key: normalized.access_key,
		secret_key: normalized.secret_key,
		base_path: normalized.base_path,
		is_default: normalized.is_default,
	};
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
	const sameDriverType = editingTarget.driver_type === normalized.driver_type;
	const payload: RemoteUpdateStorageTargetRequest = {
		name: normalized.name,
		driver_type: normalized.driver_type,
		base_path: normalized.base_path,
		is_default: normalized.is_default,
	};

	payload.endpoint = normalized.endpoint;
	payload.bucket = normalized.bucket;

	if (!supportsAccessKey) {
		payload.access_key = "";
	}
	if (!supportsSecretKey) {
		payload.secret_key = "";
	}
	if (!supportsAccessKey && !supportsSecretKey) {
		return payload;
	}

	const accessKey = normalized.access_key;
	const secretKey = normalized.secret_key;
	if (supportsAccessKey && (!sameDriverType || accessKey)) {
		payload.access_key = accessKey;
	}
	if (supportsSecretKey && (!sameDriverType || secretKey)) {
		payload.secret_key = secretKey;
	}

	return payload;
}

export const emptyRemoteStorageTargetForm: RemoteStorageTargetFormData = {
	name: "",
	driver_type: "local",
	endpoint: "",
	bucket: "",
	access_key: "",
	secret_key: "",
	base_path: ".",
	is_default: false,
};

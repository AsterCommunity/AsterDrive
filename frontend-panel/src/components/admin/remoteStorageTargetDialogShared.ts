import { normalizeObjectStorageConnectionFields } from "@/lib/objectStorageConnectionFields";
import type {
	RemoteCreateStorageTargetRequest,
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
	max_file_size: string;
	is_default: boolean;
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
		max_file_size: String(profile.max_file_size),
		is_default: profile.is_default,
	};
}

function normalizeRemoteStorageTargetForm(
	form: RemoteStorageTargetFormData,
): RemoteStorageTargetFormData {
	if (form.driver_type !== "s3") {
		return {
			...form,
			name: form.name.trim(),
			base_path: form.base_path.trim(),
			endpoint: "",
			bucket: "",
		};
	}

	const normalized = normalizeObjectStorageConnectionFields(
		form.endpoint,
		form.bucket,
	);
	return {
		...form,
		name: form.name.trim(),
		endpoint: normalized.endpoint,
		bucket: normalized.bucket,
		base_path: form.base_path.trim(),
	};
}

function parseMaxFileSize(value: string): number {
	const trimmed = value.trim();
	return trimmed === "" ? 0 : Number(trimmed);
}

export function buildCreateRemoteStorageTargetPayload(
	form: RemoteStorageTargetFormData,
): RemoteCreateStorageTargetRequest {
	const normalized = normalizeRemoteStorageTargetForm(form);
	const isS3 = normalized.driver_type === "s3";

	return {
		name: normalized.name,
		driver_type: normalized.driver_type,
		endpoint: isS3 ? normalized.endpoint.trim() : "",
		bucket: isS3 ? normalized.bucket.trim() : "",
		access_key: isS3 ? normalized.access_key.trim() : "",
		secret_key: isS3 ? normalized.secret_key.trim() : "",
		base_path: normalized.base_path,
		max_file_size: parseMaxFileSize(normalized.max_file_size),
		is_default: normalized.is_default,
	};
}

export function buildUpdateRemoteStorageTargetPayload(
	form: RemoteStorageTargetFormData,
	editingProfile: RemoteStorageTargetInfo,
): RemoteUpdateStorageTargetRequest {
	const normalized = normalizeRemoteStorageTargetForm(form);
	const isS3 = normalized.driver_type === "s3";
	const sameDriverType = editingProfile.driver_type === normalized.driver_type;
	const payload: RemoteUpdateStorageTargetRequest = {
		name: normalized.name,
		driver_type: normalized.driver_type,
		base_path: normalized.base_path,
		max_file_size: parseMaxFileSize(normalized.max_file_size),
		is_default: normalized.is_default,
	};

	if (!isS3) {
		payload.endpoint = "";
		payload.bucket = "";
		payload.access_key = "";
		payload.secret_key = "";
		return payload;
	}

	payload.endpoint = normalized.endpoint.trim();
	payload.bucket = normalized.bucket.trim();

	const accessKey = normalized.access_key.trim();
	const secretKey = normalized.secret_key.trim();
	if (!sameDriverType || accessKey) {
		payload.access_key = accessKey;
	}
	if (!sameDriverType || secretKey) {
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
	max_file_size: "0",
	is_default: false,
};

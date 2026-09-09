import { describe, expect, it } from "vitest";
import { ApiError } from "@/services/http";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import {
	getStorageConnectorFieldError,
	storageConnectorFieldErrorKey,
} from "./connectionErrors";
import type { Translate } from "./StoragePolicyFieldTypes";

const t: Translate = (key, values) => {
	if (key === "aliyun_oss_access_key_id") return "阿里云 AccessKey ID";
	if (key === "endpoint") return "端点";
	if (key === "policy_connector_field_required") {
		return `必须填写${values?.field}。`;
	}
	if (key === "policy_connector_field_invalid") {
		return `请检查${values?.field}的填写内容。`;
	}
	return key;
};

function field(
	name: string,
	scope: StorageConnectorFieldDescriptor["scope"],
): StorageConnectorFieldDescriptor {
	return {
		kind: scope === "static_credential" ? "secret" : "text",
		label_key: name,
		name,
		required: true,
		scope,
		secret: scope === "static_credential",
	};
}

const descriptor = {
	connector_id: "asterdrive.storage.alibaba_oss",
	fields: [
		field("endpoint", "connector_config"),
		field("aliyun_oss_access_key_id", "static_credential"),
	],
} as StorageConnectorDescriptor;

describe("storage connector connection errors", () => {
	it("localizes missing static credential fields using descriptor labels", () => {
		const error = new ApiError("bad_request", "raw error", {
			diagnostic: {
				kind: "connector_validation",
				message: "missing field",
				field: "aliyun_oss_access_key_id",
				scope: "static_credential",
			},
		});

		const fieldError = getStorageConnectorFieldError(error, descriptor, t);
		expect(fieldError).toEqual({
			message: "必须填写阿里云 AccessKey ID。",
			name: "aliyun_oss_access_key_id",
			scope: "static_credential",
		});
		expect(
			storageConnectorFieldErrorKey(
				fieldError?.scope ?? "static_credential",
				fieldError?.name ?? "",
			),
		).toBe("static_credential:aliyun_oss_access_key_id");
	});

	it("uses a localized invalid-value message for connector config fields", () => {
		const error = new ApiError("bad_request", "raw error", {
			diagnostic: {
				kind: "connector_validation",
				message: "must be a string",
				field: "endpoint",
				scope: "connector_config",
			},
		});

		expect(getStorageConnectorFieldError(error, descriptor, t)).toEqual({
			message: "请检查端点的填写内容。",
			name: "endpoint",
			scope: "connector_config",
		});
	});

	it("ignores diagnostics without a descriptor field or a structured API error", () => {
		const unknownField = new ApiError("bad_request", "raw error", {
			diagnostic: {
				kind: "connector_validation",
				message: "missing field",
				field: "unknown_field",
				scope: "connector_config",
			},
		});
		const unknownScope = new ApiError("bad_request", "raw error", {
			diagnostic: {
				kind: "connector_validation",
				message: "missing field",
				field: "endpoint",
				scope: "static_credential",
			},
		});

		expect(
			getStorageConnectorFieldError(unknownField, descriptor, t),
		).toBeNull();
		expect(
			getStorageConnectorFieldError(unknownScope, descriptor, t),
		).toBeNull();
		expect(
			getStorageConnectorFieldError(new Error("missing field"), descriptor, t),
		).toBeNull();
	});

	it("does not classify diagnostics that do not identify a field", () => {
		const error = new ApiError("storage.misconfigured", "raw error", {
			diagnostic: {
				kind: "misconfigured",
				message: "provider is unavailable",
			},
		});

		expect(getStorageConnectorFieldError(error, descriptor, t)).toBeNull();
	});
});

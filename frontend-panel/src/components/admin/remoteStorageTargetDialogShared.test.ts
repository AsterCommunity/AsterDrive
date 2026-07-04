import { describe, expect, it } from "vitest";
import {
	buildCreateRemoteStorageTargetPayload,
	buildUpdateRemoteStorageTargetPayload,
	getRemoteStorageTargetForm,
} from "@/components/admin/remoteStorageTargetDialogShared";
import type { RemoteStorageTargetInfo } from "@/types/api";

const localFields = new Set(["base_path", "is_default"]);
const s3Fields = new Set([
	"endpoint",
	"bucket",
	"access_key",
	"secret_key",
	"base_path",
	"is_default",
]);

describe("remoteStorageTargetDialogShared", () => {
	it("maps an existing remote storage target into form state", () => {
		expect(
			getRemoteStorageTargetForm({
				target_key: "igp_demo",
				name: "Follower Cache",
				driver_type: "local",
				endpoint: "",
				bucket: "",
				base_path: "cache/inbox",
				is_default: true,
				desired_revision: 3,
				applied_revision: 3,
				last_error: "",
				created_at: "",
				updated_at: "",
			} as RemoteStorageTargetInfo),
		).toEqual({
			name: "Follower Cache",
			driver_type: "local",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "cache/inbox",
			is_default: true,
		});
	});

	it("builds create payloads with trimmed s3 fields", () => {
		expect(
			buildCreateRemoteStorageTargetPayload(
				{
					name: "Archive",
					driver_type: "s3",
					endpoint: " https://s3.example.test/uploads ",
					bucket: " uploads ",
					access_key: "ACCESS",
					secret_key: "SECRET",
					base_path: "tenant-a/incoming",
					is_default: false,
				},
				s3Fields,
			),
		).toEqual({
			name: "Archive",
			driver_type: "s3",
			endpoint: "https://s3.example.test/uploads",
			bucket: "uploads",
			access_key: "ACCESS",
			secret_key: "SECRET",
			base_path: "tenant-a/incoming",
			is_default: false,
		});
	});

	it("omits unchanged s3 credentials from update payloads", () => {
		expect(
			buildUpdateRemoteStorageTargetPayload(
				{
					name: "Archive",
					driver_type: "s3",
					endpoint: "https://s3.example.test/uploads",
					bucket: "uploads",
					access_key: "",
					secret_key: "",
					base_path: "tenant-a/incoming",
					is_default: true,
				},
				s3Fields,
				{
					target_key: "igp_archive",
					name: "Archive",
					driver_type: "s3",
					endpoint: "https://s3.example.test",
					bucket: "uploads",
					base_path: "tenant-a/incoming",
					is_default: false,
					desired_revision: 2,
					applied_revision: 2,
					last_error: "",
					created_at: "",
					updated_at: "",
				} as RemoteStorageTargetInfo,
			),
		).toEqual({
			name: "Archive",
			driver_type: "s3",
			endpoint: "https://s3.example.test/uploads",
			bucket: "uploads",
			base_path: "tenant-a/incoming",
			is_default: true,
		});
	});

	it("requires explicit credentials when switching from local to s3", () => {
		expect(
			buildUpdateRemoteStorageTargetPayload(
				{
					name: "Promoted",
					driver_type: "s3",
					endpoint: "https://s3.example.com",
					bucket: "bucket-a",
					access_key: "ROTATED",
					secret_key: "SECRET",
					base_path: "tenant-a/incoming",
					is_default: false,
				},
				s3Fields,
				{
					target_key: "igp_local",
					name: "Promoted",
					driver_type: "local",
					endpoint: "",
					bucket: "",
					base_path: ".",
					is_default: true,
					desired_revision: 1,
					applied_revision: 1,
					last_error: "",
					created_at: "",
					updated_at: "",
				} as RemoteStorageTargetInfo,
			),
		).toEqual({
			name: "Promoted",
			driver_type: "s3",
			endpoint: "https://s3.example.com",
			bucket: "bucket-a",
			access_key: "ROTATED",
			secret_key: "SECRET",
			base_path: "tenant-a/incoming",
			is_default: false,
		});
	});

	it("clears unsupported connection fields from local payloads", () => {
		expect(
			buildCreateRemoteStorageTargetPayload(
				{
					name: "Local",
					driver_type: "local",
					endpoint: "https://unused.example.com",
					bucket: "unused",
					access_key: "unused-access",
					secret_key: "unused-secret",
					base_path: "tenant-a/local",
					is_default: true,
				},
				localFields,
			),
		).toEqual({
			name: "Local",
			driver_type: "local",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "tenant-a/local",
			is_default: true,
		});
	});
});

import { describe, expect, it } from "vitest";
import {
	buildCreateRemoteStorageTargetPayload,
	buildUpdateRemoteStorageTargetPayload,
	getRemoteStorageTargetForm,
} from "@/components/admin/remoteStorageTargetDialogShared";
import type { RemoteStorageTargetInfo } from "@/types/api";

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
				max_file_size: 2048,
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
			max_file_size: "2048",
			is_default: true,
		});
	});

	it("builds create payloads with trimmed s3 fields", () => {
		expect(
			buildCreateRemoteStorageTargetPayload({
				name: "Archive",
				driver_type: "s3",
				endpoint: " https://s3.example.test/uploads ",
				bucket: " uploads ",
				access_key: "ACCESS",
				secret_key: "SECRET",
				base_path: "tenant-a/incoming",
				max_file_size: "8192",
				is_default: false,
			}),
		).toEqual({
			name: "Archive",
			driver_type: "s3",
			endpoint: "https://s3.example.test/uploads",
			bucket: "uploads",
			access_key: "ACCESS",
			secret_key: "SECRET",
			base_path: "tenant-a/incoming",
			max_file_size: 8192,
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
					max_file_size: "",
					is_default: true,
				},
				{
					target_key: "igp_archive",
					name: "Archive",
					driver_type: "s3",
					endpoint: "https://s3.example.test",
					bucket: "uploads",
					base_path: "tenant-a/incoming",
					max_file_size: 1024,
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
			max_file_size: 0,
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
					max_file_size: "4096",
					is_default: false,
				},
				{
					target_key: "igp_local",
					name: "Promoted",
					driver_type: "local",
					endpoint: "",
					bucket: "",
					base_path: ".",
					max_file_size: 0,
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
			max_file_size: 4096,
			is_default: false,
		});
	});
});

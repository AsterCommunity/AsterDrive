import { describe, expect, it } from "vitest";
import {
	getEndpointValidationMessage,
	getPolicyConnectionTestKey,
	getS3CompatibleDriverPromotionTarget,
	hasConnectionFieldChanges,
	normalizePolicyForm,
} from "@/components/admin/storage-policy-dialog/connectionNormalization";
import {
	supportsApplicationCredentials,
	supportsContentDedupPolicyOption,
	supportsCredentialValidationAction,
	supportsDraftConnectionTest,
	supportsObjectStorageConnection,
	supportsObjectStorageTransferStrategy,
	supportsOneDrivePolicyOptions,
	supportsRemoteNodeBinding,
	supportsSavedConnectionTest,
	supportsStorageAuthorizationAction,
	supportsStorageNativeProcessing,
	supportsStoragePolicyAction,
} from "@/components/admin/storage-policy-dialog/descriptorPredicates";
import { getPolicyForm } from "@/components/admin/storage-policy-dialog/formTypes";
import {
	buildCreatePolicyPayload,
	buildPolicyTestPayload,
	buildTencentCosCorsPayload,
	buildUpdatePolicyPayload,
} from "@/components/admin/storage-policy-dialog/payloadBuilders";
import type { StoragePolicy } from "@/types/api";

describe("storage policy dialog helper modules", () => {
	const t = (key: string) => key;
	const descriptor = {
		actions: [
			{
				affordance_action: "test_saved_connection",
				endpoints: ["test_policy_connection"],
				kind: "connection_test",
				mutates_remote_state: false,
				requires_authorization: false,
				requires_saved_policy: true,
			},
		],
		capabilities: {
			remote_node_binding: false,
			storage_native_media_metadata: false,
			storage_native_thumbnail: false,
			object_storage_transfer_strategy: false,
		},
		fields: [],
		upload_workflows: {
			object_multipart_upload: false,
		},
	} as never;
	const staticSecretConnectionDescriptor = ({
		protocolMessage = "s3_endpoint_protocol_required_error",
		allowedProtocols = ["http:", "https:"],
		allowEndpointWithoutProtocol = false,
		includeS3PathStyle = false,
	}: {
		protocolMessage?: string;
		allowedProtocols?: string[];
		allowEndpointWithoutProtocol?: boolean;
		includeS3PathStyle?: boolean;
	} = {}) =>
		({
			credential_mode: "static_secret",
			fields: [
				{
					allow_endpoint_without_protocol: allowEndpointWithoutProtocol,
					allowed_endpoint_protocols: allowedProtocols,
					invalid_protocol_message_key: protocolMessage,
					name: "endpoint",
					scope: "connection",
				},
				{ name: "bucket", scope: "connection" },
				{ name: "access_key", scope: "connection" },
				{ name: "secret_key", scope: "connection" },
				{
					name: "object_storage_upload_strategy",
					scope: "policy_options",
				},
				{
					name: "object_storage_download_strategy",
					scope: "policy_options",
				},
				...(includeS3PathStyle
					? [{ name: "s3_path_style", scope: "policy_options" }]
					: []),
			],
			upload_workflows: {
				object_multipart_upload: true,
			},
		}) as never;
	const sftpConnectionDescriptor = () =>
		staticSecretConnectionDescriptor({
			allowedProtocols: ["sftp:"],
			allowEndpointWithoutProtocol: true,
			protocolMessage: "sftp_endpoint_protocol_required_error",
		});

	it("uses connector descriptor endpoint rules for specialized driver promotion", () => {
		const s3Descriptor = {
			driver_type: "s3",
			driver_recommendations: [
				{
					target_driver_type: "tencent_cos",
					endpoint_host_rules: [
						{ equals: "myqcloud.com" },
						{ ends_with: ".myqcloud.com" },
					],
				},
			],
		} as never;
		const labelFor = (driverType: "tencent_cos") =>
			driverType === "tencent_cos" ? "Tencent COS" : driverType;
		expect(
			getS3CompatibleDriverPromotionTarget(
				{
					driver_type: "s3",
					endpoint: "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
				},
				s3Descriptor,
				labelFor,
			),
		).toEqual({
			driverLabel: "Tencent COS",
			driverType: "tencent_cos",
		});
		expect(
			getS3CompatibleDriverPromotionTarget(
				{
					driver_type: "s3",
					endpoint: "https://myqcloud.com",
				},
				s3Descriptor,
				labelFor,
			),
		).toEqual({
			driverLabel: "Tencent COS",
			driverType: "tencent_cos",
		});
		expect(
			getS3CompatibleDriverPromotionTarget(
				{
					driver_type: "s3",
					endpoint: "https://s3.amazonaws.com",
				},
				s3Descriptor,
				labelFor,
			),
		).toBeNull();
		expect(
			getS3CompatibleDriverPromotionTarget(
				{
					driver_type: "tencent_cos",
					endpoint: "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
				},
				s3Descriptor,
				labelFor,
			),
		).toBeNull();
		expect(
			getS3CompatibleDriverPromotionTarget(
				{
					driver_type: "s3",
					endpoint: "not a url",
				},
				s3Descriptor,
				labelFor,
			),
		).toBeNull();
		expect(
			getS3CompatibleDriverPromotionTarget(
				{
					driver_type: "s3",
					endpoint: "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
				},
				{ driver_type: "s3", driver_recommendations: [] } as never,
				labelFor,
			),
		).toBeNull();
		expect(
			getS3CompatibleDriverPromotionTarget(null, s3Descriptor, labelFor),
		).toBeNull();
	});

	it("uses backend storage driver descriptors as feature gate source", () => {
		const objectStorageDescriptor = {
			capabilities: {
				remote_node_binding: false,
				object_storage_transfer_strategy: true,
			},
			fields: [
				{ name: "endpoint", scope: "connection" },
				{ name: "bucket", scope: "connection" },
				{ name: "access_key", scope: "connection" },
				{ name: "secret_key", scope: "connection" },
			],
			upload_workflows: {
				object_multipart_upload: true,
			},
		} as never;
		const remoteDescriptor = {
			capabilities: {
				remote_node_binding: true,
			},
			fields: [{ name: "remote_node_id", scope: "remote_node_binding" }],
			upload_workflows: {},
		} as never;
		const onedriveDescriptor = {
			actions: [
				{ affordance_action: "start_authorization", kind: "authorization" },
				{
					affordance_action: "validate_credential",
					kind: "credential_validation",
				},
			],
			capabilities: {
				remote_node_binding: false,
			},
			fields: [{ name: "account_mode", scope: "policy_options" }],
			upload_workflows: {},
		} as never;
		const contentDedupDescriptor = {
			capabilities: {
				remote_node_binding: false,
			},
			fields: [{ name: "content_dedup", scope: "policy_options" }],
			upload_workflows: {},
		} as never;

		expect(supportsDraftConnectionTest()).toBe(false);
		expect(supportsDraftConnectionTest(descriptor)).toBe(false);
		expect(supportsSavedConnectionTest()).toBe(false);
		expect(supportsSavedConnectionTest(descriptor)).toBe(true);
		expect(supportsStorageNativeProcessing()).toBe(false);
		expect(supportsStorageNativeProcessing(descriptor)).toBe(false);
		expect(
			supportsStoragePolicyAction(descriptor, "configure_tencent_cos_cors"),
		).toBe(false);
		expect(
			supportsStoragePolicyAction(null, "configure_tencent_cos_cors"),
		).toBe(false);
		expect(supportsObjectStorageConnection(objectStorageDescriptor)).toBe(true);
		expect(supportsRemoteNodeBinding(remoteDescriptor)).toBe(true);
		expect(supportsObjectStorageTransferStrategy(objectStorageDescriptor)).toBe(
			true,
		);
		expect(supportsOneDrivePolicyOptions(onedriveDescriptor)).toBe(true);
		expect(supportsContentDedupPolicyOption(contentDedupDescriptor)).toBe(true);
		expect(supportsStorageAuthorizationAction(onedriveDescriptor)).toBe(true);
		expect(supportsCredentialValidationAction(onedriveDescriptor)).toBe(true);
	});

	it("rejects incomplete or mismatched descriptor feature gates", () => {
		const incompleteObjectStorageDescriptor = {
			capabilities: {
				remote_node_binding: false,
				object_storage_transfer_strategy: true,
			},
			fields: [
				{ name: "endpoint", scope: "connection" },
				{ name: "bucket", scope: "connection" },
				{ name: "access_key", scope: "connection" },
			],
			upload_workflows: {
				object_multipart_upload: true,
			},
		} as never;
		const wrongScopeObjectStorageDescriptor = {
			capabilities: {
				remote_node_binding: false,
				object_storage_transfer_strategy: true,
			},
			fields: [
				{ name: "endpoint", scope: "policy_options" },
				{ name: "bucket", scope: "connection" },
				{ name: "access_key", scope: "connection" },
				{ name: "secret_key", scope: "connection" },
			],
			upload_workflows: {
				object_multipart_upload: true,
			},
		} as never;
		const noMultipartWorkflowDescriptor = {
			capabilities: {
				remote_node_binding: false,
				object_storage_transfer_strategy: true,
			},
			fields: [
				{ name: "endpoint", scope: "connection" },
				{ name: "bucket", scope: "connection" },
				{ name: "access_key", scope: "connection" },
				{ name: "secret_key", scope: "connection" },
			],
			upload_workflows: {
				object_multipart_upload: false,
			},
		} as never;
		const applicationCredentialDescriptor = {
			capabilities: {
				remote_node_binding: false,
			},
			fields: [{ name: "client_id", scope: "application_credential" }],
			upload_workflows: {},
		} as never;

		expect(supportsObjectStorageConnection(null)).toBe(false);
		expect(
			supportsObjectStorageConnection(incompleteObjectStorageDescriptor),
		).toBe(false);
		expect(
			supportsObjectStorageConnection(wrongScopeObjectStorageDescriptor),
		).toBe(false);
		expect(supportsObjectStorageConnection(noMultipartWorkflowDescriptor)).toBe(
			false,
		);
		expect(supportsRemoteNodeBinding(null)).toBe(false);
		expect(supportsObjectStorageTransferStrategy(null)).toBe(false);
		expect(supportsStorageAuthorizationAction(null)).toBe(false);
		expect(supportsCredentialValidationAction(null)).toBe(false);
		expect(supportsOneDrivePolicyOptions(applicationCredentialDescriptor)).toBe(
			false,
		);
		expect(
			supportsApplicationCredentials(applicationCredentialDescriptor),
		).toBe(true);
		expect(supportsApplicationCredentials(null)).toBe(false);
		expect(supportsContentDedupPolicyOption(null)).toBe(false);
		expect(
			supportsContentDedupPolicyOption({
				fields: [{ name: "content_dedup", scope: "connection" }],
			} as never),
		).toBe(false);
	});

	it("maps an existing policy into form state", () => {
		expect(
			getPolicyForm({
				id: 3,
				name: "Archive",
				driver_type: "local",
				endpoint: "",
				bucket: "",
				access_key: "",
				secret_key: "",
				base_path: "/data/archive",
				remote_node_id: null,
				max_file_size: 1024,
				allowed_types: [],
				options: {
					content_dedup: true,
					storage_native_processing_enabled: true,
					thumbnail_processor: "storage_native",
					thumbnail_extensions: ["png", "jpg"],
				},
				is_default: true,
				chunk_size: 10 * 1024 * 1024,
				created_at: "",
				updated_at: "",
			} as StoragePolicy),
		).toEqual({
			name: "Archive",
			driver_type: "local",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "/data/archive",
			remote_node_id: "",
			remote_storage_target_key: "",
			max_file_size: "1024",
			chunk_size: "10",
			is_default: true,
			content_dedup: true,
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			object_storage_upload_strategy: "relay_stream",
			object_storage_download_strategy: "relay_stream",
			s3_path_style: true,
			onedrive_cloud: "global",
			onedrive_account_mode: "work_or_school",
			onedrive_tenant: "common",
			onedrive_drive_id: "",
			onedrive_root_item_id: "",
			onedrive_site_id: "",
			onedrive_group_id: "",
			policy_option_values: {
				content_dedup: "true",
				storage_native_processing_enabled: "true",
				thumbnail_processor: "storage_native",
			},
			application_credentials: {
				microsoft_graph: {
					cloud: "global",
					tenant: "common",
					client_id: "",
					client_secret: "",
					scopes: "",
				},
			},
			storage_native_processing_enabled: true,
			storage_native_media_metadata_enabled: false,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png", "jpg"],
			media_metadata_extensions: [],
		});
	});

	it("builds create payloads with trimmed S3 fields", () => {
		const s3Descriptor = staticSecretConnectionDescriptor({
			includeS3PathStyle: true,
		});
		expect(
			buildCreatePolicyPayload(
				{
					name: "Media",
					driver_type: "s3",
					endpoint: " https://s3.example.test/custom/path ",
					bucket: " photos ",
					access_key: "AKIA",
					secret_key: "SECRET",
					base_path: "videos",
					remote_node_id: "",
					max_file_size: "2048",
					chunk_size: "6",
					is_default: false,
					content_dedup: false,
					remote_download_strategy: "relay_stream",
					remote_upload_strategy: "relay_stream",
					object_storage_upload_strategy: "presigned",
					object_storage_download_strategy: "relay_stream",
					storage_native_processing_enabled: false,
					thumbnail_processor: null,
					thumbnail_extensions: [],
				},
				s3Descriptor,
			),
		).toEqual({
			name: "Media",
			driver_type: "s3",
			endpoint: "https://s3.example.test/custom/path",
			bucket: "photos",
			access_key: "AKIA",
			secret_key: "SECRET",
			base_path: "videos",
			max_file_size: 2048,
			chunk_size: 6 * 1024 * 1024,
			is_default: false,
			options: {
				object_storage_download_strategy: "relay_stream",
				object_storage_upload_strategy: "presigned",
			},
			remote_node_id: undefined,
			remote_storage_target_key: undefined,
		});

		expect(
			buildCreatePolicyPayload(
				{
					name: "Virtual Hosted S3",
					driver_type: "s3",
					endpoint: "https://s3.example.test",
					bucket: "photos",
					access_key: "AKIA",
					secret_key: "SECRET",
					base_path: "",
					remote_node_id: "",
					max_file_size: "",
					chunk_size: "5",
					is_default: false,
					content_dedup: false,
					remote_download_strategy: "relay_stream",
					remote_upload_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
					object_storage_download_strategy: "relay_stream",
					s3_path_style: false,
					storage_native_processing_enabled: false,
					thumbnail_processor: null,
					thumbnail_extensions: [],
				},
				s3Descriptor,
			).options,
		).toEqual({
			object_storage_upload_strategy: "relay_stream",
			object_storage_download_strategy: "relay_stream",
			s3_path_style: false,
		});
	});

	it("stores OneDrive Microsoft app settings under generic application config", () => {
		const form = getPolicyForm({
			id: 12,
			name: "Graph Drive",
			driver_type: "one_drive",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "teams/archive",
			remote_node_id: null,
			max_file_size: 0,
			allowed_types: [],
			options: {
				onedrive_cloud: "china",
				onedrive_account_mode: "sharepoint_site",
				onedrive_tenant: "contoso.partner.onmschina.cn",
				onedrive_drive_id: "drive-1",
				onedrive_root_item_id: "root-item-1",
				onedrive_site_id: "site-1",
			},
			is_default: false,
			chunk_size: 10 * 1024 * 1024,
			created_at: "",
			updated_at: "",
		} as StoragePolicy);

		expect(form).toMatchObject({
			driver_type: "one_drive",
			onedrive_cloud: "china",
			onedrive_account_mode: "sharepoint_site",
			onedrive_tenant: "contoso.partner.onmschina.cn",
			onedrive_drive_id: "drive-1",
			onedrive_root_item_id: "root-item-1",
			onedrive_site_id: "site-1",
			application_credentials: {
				microsoft_graph: {
					cloud: "china",
					tenant: "contoso.partner.onmschina.cn",
					client_id: "",
					client_secret: "",
					scopes: "",
				},
			},
		});

		const descriptor = {
			fields: [
				{ name: "client_id", scope: "application_credential" },
				{ name: "account_mode", scope: "policy_options" },
			],
		} as never;

		expect(
			buildCreatePolicyPayload(
				{
					...form,
					application_credentials: {
						microsoft_graph: {
							cloud: "china",
							tenant: "contoso.partner.onmschina.cn",
							client_id: "client-id",
							client_secret: "secret",
							scopes: "Files.ReadWrite.All offline_access",
						},
					},
				},
				descriptor,
			),
		).toMatchObject({
			access_key: "",
			secret_key: "",
			application_config: {
				microsoft_graph: {
					cloud: "china",
					tenant: "contoso.partner.onmschina.cn",
					client_id: "client-id",
					client_secret: "secret",
					scopes: ["Files.ReadWrite.All", "offline_access"],
				},
			},
			options: {
				onedrive_cloud: "china",
				onedrive_account_mode: "sharepoint_site",
				onedrive_tenant: "contoso.partner.onmschina.cn",
				onedrive_drive_id: "drive-1",
				onedrive_root_item_id: "root-item-1",
				onedrive_site_id: "site-1",
			},
		});

		expect(
			buildUpdatePolicyPayload(
				{
					...form,
					application_credentials: {
						microsoft_graph: {
							cloud: "china",
							tenant: "contoso.partner.onmschina.cn",
							client_id: "new-client-id",
							client_secret: "new-secret",
							scopes: "",
						},
					},
				},
				descriptor,
			),
		).toMatchObject({
			application_config: {
				microsoft_graph: {
					cloud: "china",
					tenant: "contoso.partner.onmschina.cn",
					client_id: "new-client-id",
					client_secret: "new-secret",
				},
			},
		});

		expect(
			buildUpdatePolicyPayload(
				{
					...form,
					onedrive_tenant: " organizations ",
					onedrive_drive_id: " ",
					onedrive_root_item_id: " ",
					onedrive_site_id: " ",
					onedrive_group_id: " ",
				},
				descriptor,
			).options,
		).toEqual({
			onedrive_cloud: "china",
			onedrive_account_mode: "sharepoint_site",
			onedrive_tenant: "organizations",
			onedrive_root_item_id: "root",
		});

		expect(
			buildUpdatePolicyPayload(
				{
					...form,
					onedrive_account_mode: "work_or_school",
					onedrive_site_id: "stale-site",
					onedrive_group_id: "stale-group",
				},
				descriptor,
			).options,
		).toEqual({
			onedrive_cloud: "china",
			onedrive_account_mode: "work_or_school",
			onedrive_tenant: "contoso.partner.onmschina.cn",
			onedrive_drive_id: "drive-1",
			onedrive_root_item_id: "root-item-1",
		});

		expect(
			buildUpdatePolicyPayload(
				{
					...form,
					onedrive_account_mode: "group_drive",
					onedrive_site_id: "stale-site",
					onedrive_group_id: "group-1",
				},
				descriptor,
			).options,
		).toMatchObject({
			onedrive_account_mode: "group_drive",
			onedrive_group_id: "group-1",
		});
		expect(
			buildUpdatePolicyPayload(
				{
					...form,
					onedrive_account_mode: "group_drive",
					onedrive_site_id: "stale-site",
					onedrive_group_id: "group-1",
				},
				descriptor,
			).options,
		).not.toHaveProperty("onedrive_site_id");

		expect(
			buildUpdatePolicyPayload(
				{
					...form,
					onedrive_account_mode: "sharepoint_site",
					onedrive_site_id: "site-1",
					onedrive_group_id: "stale-group",
				},
				descriptor,
			).options,
		).not.toHaveProperty("onedrive_group_id");

		expect(
			buildUpdatePolicyPayload(
				{
					...form,
				},
				descriptor,
			),
		).not.toHaveProperty("secret_key");
	});

	it("validates S3-compatible endpoint protocols without blocking remote policies", () => {
		const baseForm = {
			name: "Media",
			driver_type: "s3" as const,
			endpoint: "s3.example.com",
			bucket: "bucket-a",
			access_key: "",
			secret_key: "",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			object_storage_upload_strategy: "relay_stream" as const,
			object_storage_download_strategy: "relay_stream" as const,
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			media_metadata_extensions: [],
		};
		const s3Descriptor = staticSecretConnectionDescriptor();

		expect(getEndpointValidationMessage(baseForm, t, s3Descriptor)).toBe(
			"s3_endpoint_protocol_required_error",
		);
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "http://s3.example.com" },
				t,
				s3Descriptor,
			),
		).toBeNull();
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "https://s3.example.com" },
				t,
				s3Descriptor,
			),
		).toBeNull();
		expect(
			getEndpointValidationMessage(
				{
					...baseForm,
					driver_type: "tencent_cos",
					endpoint: "cos.ap-guangzhou.myqcloud.com",
				},
				t,
				s3Descriptor,
			),
		).toBe("s3_endpoint_protocol_required_error");
		expect(
			getEndpointValidationMessage(
				{
					...baseForm,
					endpoint: "ftp://s3.example.com",
					bucket: "",
					access_key: "",
					secret_key: "",
				},
				t,
				s3Descriptor,
			),
		).toBe("s3_endpoint_protocol_required_error");
		expect(getEndpointValidationMessage(baseForm, t, null)).toBeNull();
		expect(
			getEndpointValidationMessage(
				{
					...baseForm,
					driver_type: "remote",
					endpoint: "edge-node-without-protocol",
					bucket: "",
				},
				t,
			),
		).toBeNull();
	});

	it("validates Azure Blob endpoints separately from S3-compatible drivers", () => {
		const baseForm = {
			name: "Azure Blob",
			driver_type: "azure_blob" as const,
			endpoint: "acct.blob.core.windows.net",
			bucket: "container-a",
			access_key: "account-name",
			secret_key: "account-key",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			object_storage_upload_strategy: "presigned" as const,
			object_storage_download_strategy: "relay_stream" as const,
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			media_metadata_extensions: [],
		};
		const azureDescriptor = staticSecretConnectionDescriptor({
			protocolMessage: "azure_blob_endpoint_protocol_required_error",
		});

		expect(getEndpointValidationMessage(baseForm, t, azureDescriptor)).toBe(
			"azure_blob_endpoint_protocol_required_error",
		);
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "https://acct.blob.core.windows.net" },
				t,
				azureDescriptor,
			),
		).toBeNull();
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "ftp://acct.blob.core.windows.net" },
				t,
				azureDescriptor,
			),
		).toBe("azure_blob_endpoint_protocol_required_error");
		expect(
			getEndpointValidationMessage(
				{
					...baseForm,
					driver_type: "s3",
					endpoint: "ftp://s3.example.com",
				},
				t,
				staticSecretConnectionDescriptor(),
			),
		).toBe("s3_endpoint_protocol_required_error");
	});

	it("treats bare SFTP host and host port endpoints as scheme-less", () => {
		const baseForm = {
			name: "SFTP",
			driver_type: "sftp" as const,
			endpoint: "sftp.example.com:2222",
			bucket: "",
			access_key: "aster",
			secret_key: "secret",
			base_path: "/upload",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			object_storage_upload_strategy: "relay_stream" as const,
			object_storage_download_strategy: "relay_stream" as const,
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			media_metadata_extensions: [],
		};
		const descriptor = sftpConnectionDescriptor();

		expect(getEndpointValidationMessage(baseForm, t, descriptor)).toBeNull();
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "sftp://sftp.example.com:2222" },
				t,
				descriptor,
			),
		).toBeNull();
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "https://sftp.example.com" },
				t,
				descriptor,
			),
		).toBe("sftp_endpoint_protocol_required_error");
	});

	it("builds Tencent COS CORS draft action payloads with optional saved policy reuse", () => {
		const form = {
			name: "COS Media",
			driver_type: "tencent_cos" as const,
			endpoint: " https://cos.ap-guangzhou.myqcloud.com ",
			bucket: " media-1250000000 ",
			access_key: "",
			secret_key: "",
			base_path: "tenant-a",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			object_storage_upload_strategy: "presigned" as const,
			object_storage_download_strategy: "presigned" as const,
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native" as const,
			thumbnail_extensions: [" .PNG ", "jpg"],
			storage_native_media_metadata_enabled: true,
			media_metadata_extensions: [" mp4 "],
		};
		const tencentCosDescriptor = staticSecretConnectionDescriptor();

		expect(buildTencentCosCorsPayload(form, 34, tencentCosDescriptor)).toEqual({
			action: "configure_tencent_cos_cors",
			policy_id: 34,
			driver_type: "tencent_cos",
			endpoint: "https://cos.ap-guangzhou.myqcloud.com",
			bucket: "media-1250000000",
			access_key: undefined,
			secret_key: undefined,
			base_path: "tenant-a",
			remote_node_id: undefined,
			remote_storage_target_key: undefined,
			options: {
				object_storage_upload_strategy: "presigned",
				object_storage_download_strategy: "presigned",
				storage_native_processing_enabled: true,
				thumbnail_processor: "storage_native",
				thumbnail_extensions: ["png", "jpg"],
				storage_native_media_metadata_enabled: true,
				media_metadata_extensions: ["mp4"],
			},
		});
		expect(
			buildTencentCosCorsPayload(form, null, tencentCosDescriptor).policy_id,
		).toBeUndefined();
	});

	it("omits empty credentials from update payloads", () => {
		expect(
			buildUpdatePolicyPayload({
				name: "Media",
				driver_type: "s3",
				endpoint: "https://example.com",
				bucket: "bucket-a",
				access_key: "",
				secret_key: "",
				base_path: "videos",
				remote_node_id: "",
				max_file_size: "",
				chunk_size: "5",
				is_default: true,
				content_dedup: false,
				remote_download_strategy: "relay_stream",
				remote_upload_strategy: "relay_stream",
				object_storage_upload_strategy: "relay_stream",
				object_storage_download_strategy: "presigned",
				storage_native_processing_enabled: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}),
		).toEqual({
			name: "Media",
			endpoint: "https://example.com",
			bucket: "bucket-a",
			base_path: "videos",
			max_file_size: undefined,
			chunk_size: 5 * 1024 * 1024,
			is_default: true,
			options: {
				object_storage_download_strategy: "presigned",
			},
			remote_node_id: undefined,
		});
	});

	it("builds remote payloads with remote node binding and storage target", () => {
		expect(
			buildCreatePolicyPayload({
				name: "Remote Edge",
				driver_type: "remote",
				endpoint: "",
				bucket: "",
				access_key: "",
				secret_key: "",
				base_path: "tenant-a/uploads",
				remote_node_id: "9",
				remote_storage_target_key: "rst_hot",
				max_file_size: "",
				chunk_size: "4",
				is_default: false,
				content_dedup: false,
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
				object_storage_upload_strategy: "relay_stream",
				object_storage_download_strategy: "relay_stream",
				storage_native_processing_enabled: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}),
		).toEqual({
			name: "Remote Edge",
			driver_type: "remote",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "tenant-a/uploads",
			remote_node_id: 9,
			remote_storage_target_key: "rst_hot",
			max_file_size: undefined,
			chunk_size: 4 * 1024 * 1024,
			is_default: false,
			options: {
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
			},
		});

		expect(
			buildPolicyTestPayload({
				name: "Remote Edge",
				driver_type: "remote",
				endpoint: "",
				bucket: "",
				access_key: "",
				secret_key: "",
				base_path: "tenant-a/uploads",
				remote_node_id: "9",
				remote_storage_target_key: "rst_hot",
				max_file_size: "",
				chunk_size: "4",
				is_default: false,
				content_dedup: false,
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
				object_storage_upload_strategy: "relay_stream",
				object_storage_download_strategy: "relay_stream",
				storage_native_processing_enabled: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}),
		).toEqual({
			driver_type: "remote",
			endpoint: undefined,
			bucket: undefined,
			access_key: undefined,
			secret_key: undefined,
			base_path: "tenant-a/uploads",
			remote_node_id: 9,
			remote_storage_target_key: "rst_hot",
			options: {
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
			},
		});

		expect(
			buildPolicyTestPayload(
				{
					name: "Remote Edge",
					driver_type: "remote",
					endpoint: "",
					bucket: "",
					access_key: "",
					secret_key: "",
					base_path: "tenant-a/uploads",
					remote_node_id: "9",
					remote_storage_target_key: "rst_hot",
					max_file_size: "",
					chunk_size: "4",
					is_default: false,
					content_dedup: false,
					remote_download_strategy: "presigned",
					remote_upload_strategy: "presigned",
					object_storage_upload_strategy: "relay_stream",
					object_storage_download_strategy: "relay_stream",
					storage_native_processing_enabled: false,
					thumbnail_processor: null,
					thumbnail_extensions: [],
				},
				null,
				34,
			),
		).toMatchObject({
			policy_id: 34,
			driver_type: "remote",
		});

		expect(
			buildPolicyTestPayload(
				{
					name: "Remote Edge",
					driver_type: "remote",
					endpoint: "",
					bucket: "",
					access_key: "",
					secret_key: "",
					base_path: "tenant-a/uploads",
					remote_node_id: "9",
					remote_storage_target_key: "rst_hot",
					max_file_size: "",
					chunk_size: "4",
					is_default: false,
					content_dedup: false,
					remote_download_strategy: "presigned",
					remote_upload_strategy: "presigned",
					object_storage_upload_strategy: "relay_stream",
					object_storage_download_strategy: "relay_stream",
					storage_native_processing_enabled: false,
					thumbnail_processor: null,
					thumbnail_extensions: [],
				},
				null,
				null,
			),
		).not.toHaveProperty("policy_id");
	});

	it("builds Azure Blob payloads with object-storage options but without S3 path style", () => {
		const azureForm = {
			name: "Azure Archive",
			driver_type: "azure_blob" as const,
			endpoint: " https://acct.blob.core.windows.net/ ",
			bucket: " container-a ",
			access_key: " account-name ",
			secret_key: " account-key ",
			base_path: "archives",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "8",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			object_storage_upload_strategy: "presigned" as const,
			object_storage_download_strategy: "relay_stream" as const,
			s3_path_style: false,
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			media_metadata_extensions: [],
		};
		const azureDescriptor = staticSecretConnectionDescriptor({
			protocolMessage: "azure_blob_endpoint_protocol_required_error",
		});
		const payload = buildCreatePolicyPayload(
			{
				...azureForm,
			},
			azureDescriptor,
		);

		expect(payload).toEqual({
			name: "Azure Archive",
			driver_type: "azure_blob",
			endpoint: "https://acct.blob.core.windows.net/",
			bucket: "container-a",
			access_key: "account-name",
			secret_key: "account-key",
			base_path: "archives",
			remote_node_id: undefined,
			max_file_size: undefined,
			chunk_size: 8 * 1024 * 1024,
			is_default: false,
			options: {
				object_storage_download_strategy: "relay_stream",
				object_storage_upload_strategy: "presigned",
			},
			remote_storage_target_key: undefined,
		});
		expect(normalizePolicyForm(azureForm, azureDescriptor)).toEqual({
			...azureForm,
			endpoint: "https://acct.blob.core.windows.net/",
			bucket: "container-a",
			access_key: "account-name",
			secret_key: "account-key",
		});
		expect(getPolicyConnectionTestKey(azureForm, azureDescriptor)).toBe(
			JSON.stringify({
				driver_type: "azure_blob",
				endpoint: "https://acct.blob.core.windows.net/",
				bucket: "container-a",
				access_key: "account-name",
				secret_key: "account-key",
				base_path: "archives",
				remote_node_id: undefined,
				options: {
					object_storage_upload_strategy: "presigned",
					object_storage_download_strategy: "relay_stream",
				},
			}),
		);

		expect(
			hasConnectionFieldChanges(
				{
					...getPolicyForm({
						id: 15,
						name: "Azure Archive",
						driver_type: "azure_blob",
						endpoint: "https://acct.blob.core.windows.net",
						bucket: "container-a",
						access_key: "",
						secret_key: "",
						base_path: "archives",
						remote_node_id: null,
						max_file_size: null,
						allowed_types: [],
						options: {
							object_storage_upload_strategy: "presigned",
							object_storage_download_strategy: "relay_stream",
						},
						is_default: false,
						chunk_size: 8 * 1024 * 1024,
						created_at: "",
						updated_at: "",
					} as StoragePolicy),
					object_storage_upload_strategy: "relay_stream",
				},
				{
					id: 15,
					name: "Azure Archive",
					driver_type: "azure_blob",
					endpoint: "https://acct.blob.core.windows.net",
					bucket: "container-a",
					access_key: "",
					secret_key: "",
					base_path: "archives",
					remote_node_id: null,
					max_file_size: null,
					allowed_types: [],
					options: {
						object_storage_upload_strategy: "presigned",
						object_storage_download_strategy: "relay_stream",
					},
					is_default: false,
					chunk_size: 8 * 1024 * 1024,
					created_at: "",
					updated_at: "",
				} as StoragePolicy,
				azureDescriptor,
			),
		).toBe(false);
	});

	it("preserves policy-level thumbnail options in create and update payloads", () => {
		const form = {
			name: "Native Thumbnails",
			driver_type: "remote" as const,
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "tenant-a/uploads",
			remote_node_id: "9",
			max_file_size: "",
			chunk_size: "4",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "presigned" as const,
			remote_upload_strategy: "presigned" as const,
			object_storage_upload_strategy: "relay_stream" as const,
			object_storage_download_strategy: "relay_stream" as const,
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native" as const,
			thumbnail_extensions: ["png", "jpg"],
		};

		expect(buildCreatePolicyPayload(form).options).toEqual({
			remote_download_strategy: "presigned",
			remote_upload_strategy: "presigned",
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png", "jpg"],
		});
		expect(buildUpdatePolicyPayload(form).options).toEqual({
			remote_download_strategy: "presigned",
			remote_upload_strategy: "presigned",
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png", "jpg"],
		});
	});

	it("serializes policy options from connector descriptor fields when descriptors are present", () => {
		const remoteForm = {
			name: "Remote",
			driver_type: "remote" as const,
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "",
			remote_node_id: "9",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: true,
			remote_download_strategy: "presigned" as const,
			remote_upload_strategy: "presigned" as const,
			object_storage_upload_strategy: "presigned" as const,
			object_storage_download_strategy: "presigned" as const,
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
		};
		const remoteDescriptor = {
			fields: [
				{ name: "remote_download_strategy", scope: "policy_options" },
				{ name: "remote_upload_strategy", scope: "policy_options" },
			],
		} as never;
		const descriptorWithoutRemoteUpload = {
			fields: [{ name: "remote_download_strategy", scope: "policy_options" }],
		} as never;

		expect(
			buildCreatePolicyPayload(remoteForm, remoteDescriptor).options,
		).toEqual({
			remote_download_strategy: "presigned",
			remote_upload_strategy: "presigned",
		});
		expect(
			buildCreatePolicyPayload(remoteForm, descriptorWithoutRemoteUpload)
				.options,
		).toEqual({
			remote_download_strategy: "presigned",
		});

		const s3Form = {
			...remoteForm,
			driver_type: "s3" as const,
			endpoint: "https://s3.example.com",
			bucket: "bucket",
			access_key: "AKID",
			secret_key: "SECRET",
			remote_node_id: "",
			s3_path_style: false,
		};
		const s3Descriptor = {
			fields: [
				{ name: "object_storage_upload_strategy", scope: "policy_options" },
				{ name: "object_storage_download_strategy", scope: "policy_options" },
				{ name: "s3_path_style", scope: "policy_options" },
			],
		} as never;
		const azureDescriptor = {
			fields: [
				{ name: "object_storage_upload_strategy", scope: "policy_options" },
				{ name: "object_storage_download_strategy", scope: "policy_options" },
			],
		} as never;

		expect(buildCreatePolicyPayload(s3Form, s3Descriptor).options).toEqual({
			object_storage_upload_strategy: "presigned",
			object_storage_download_strategy: "presigned",
			s3_path_style: false,
		});
		expect(buildCreatePolicyPayload(s3Form, azureDescriptor).options).toEqual({
			object_storage_upload_strategy: "presigned",
			object_storage_download_strategy: "presigned",
		});

		const legacyObjectStorageDescriptor = {
			fields: [
				{ name: "s3_upload_strategy", scope: "policy_options" },
				{ name: "s3_download_strategy", scope: "policy_options" },
			],
		} as never;
		expect(
			buildCreatePolicyPayload(s3Form, legacyObjectStorageDescriptor).options,
		).toEqual({
			object_storage_upload_strategy: "presigned",
			object_storage_download_strategy: "presigned",
		});

		const sftpDescriptor = {
			fields: [
				{
					name: "sftp_host_key_fingerprint",
					kind: "text",
					scope: "policy_options",
					trim_on_blur: true,
				},
				{
					name: "api_secret",
					kind: "secret",
					scope: "policy_options",
				},
			],
		} as never;
		expect(
			buildCreatePolicyPayload(
				{
					...remoteForm,
					driver_type: "sftp" as const,
					endpoint: "sftp.example.com:22",
					access_key: "aster",
					secret_key: "secret",
					policy_option_values: {
						api_secret: "  keep-space  ",
						sftp_host_key_fingerprint: " SHA256:trusted ",
					},
				},
				sftpDescriptor,
			).options,
		).toEqual({
			api_secret: "  keep-space  ",
			sftp_host_key_fingerprint: "SHA256:trusted",
		});
	});

	it("uses application credential descriptor fields for Microsoft Graph app config", () => {
		const form = {
			name: "Graph",
			driver_type: "one_drive" as const,
			endpoint: "",
			bucket: "",
			access_key: "legacy",
			secret_key: "legacy-secret",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			object_storage_upload_strategy: "relay_stream" as const,
			object_storage_download_strategy: "relay_stream" as const,
			onedrive_cloud: "global" as const,
			onedrive_account_mode: "work_or_school" as const,
			onedrive_tenant: " common ",
			onedrive_drive_id: "",
			onedrive_root_item_id: "",
			onedrive_site_id: "",
			onedrive_group_id: "",
			application_credentials: {
				microsoft_graph: {
					cloud: "global" as const,
					tenant: " common ",
					client_id: " client-id ",
					client_secret: " secret ",
					scopes: "Files.ReadWrite.All Files.ReadWrite.All",
				},
			},
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
		};
		const descriptor = {
			fields: [
				{ name: "client_id", scope: "application_credential" },
				{ name: "account_mode", scope: "policy_options" },
			],
		} as never;

		expect(buildCreatePolicyPayload(form, descriptor)).toMatchObject({
			access_key: "",
			secret_key: "",
			application_config: {
				microsoft_graph: {
					client_id: "client-id",
					client_secret: "secret",
					tenant: "common",
					scopes: ["Files.ReadWrite.All"],
				},
			},
			options: {
				onedrive_cloud: "global",
				onedrive_account_mode: "work_or_school",
				onedrive_tenant: "common",
				onedrive_root_item_id: "root",
			},
		});
		expect(
			buildCreatePolicyPayload(form, { fields: [] } as never),
		).not.toHaveProperty("application_config");
	});

	it("keeps storage-native thumbnail suffixes independent per policy", () => {
		const baseForm = {
			name: "COS Native",
			driver_type: "tencent_cos" as const,
			endpoint: "https://cos.ap-guangzhou.myqcloud.com",
			bucket: "bucket-1250000000",
			access_key: "AKID",
			secret_key: "SECRET",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			object_storage_upload_strategy: "relay_stream" as const,
			object_storage_download_strategy: "relay_stream" as const,
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native" as const,
			thumbnail_extensions: [" .PNG ", "jpg", ".png", "../../etc/passwd"],
		};

		expect(buildCreatePolicyPayload(baseForm).options).toEqual({
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png", "jpg"],
		});
		expect(
			buildCreatePolicyPayload({
				...baseForm,
				name: "COS WebP",
				thumbnail_extensions: ["webp", "gif"],
			}).options,
		).toEqual({
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["webp", "gif"],
		});
	});

	it("does not persist any storage-native options when storage-native processing is disabled", () => {
		const payload = buildCreatePolicyPayload({
			name: "Plain S3",
			driver_type: "s3",
			endpoint: "https://s3.example.com",
			bucket: "bucket",
			access_key: "AKID",
			secret_key: "SECRET",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			object_storage_upload_strategy: "relay_stream",
			object_storage_download_strategy: "relay_stream",
			storage_native_processing_enabled: false,
			storage_native_media_metadata_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png"],
			media_metadata_extensions: ["mp4"],
		});

		expect(payload.options).toEqual({});
	});

	it("preserves storage-native media metadata switch with empty suffixes", () => {
		const payload = buildCreatePolicyPayload({
			name: "COS Metadata",
			driver_type: "tencent_cos",
			endpoint: "https://cos.ap-guangzhou.myqcloud.com",
			bucket: "bucket-1250000000",
			access_key: "AKID",
			secret_key: "SECRET",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			object_storage_upload_strategy: "relay_stream",
			object_storage_download_strategy: "relay_stream",
			storage_native_processing_enabled: true,
			storage_native_media_metadata_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["jpg"],
			media_metadata_extensions: [],
		});

		expect(payload.options).toEqual({
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["jpg"],
			storage_native_media_metadata_enabled: true,
		});
	});

	it("normalizes storage-native media metadata suffixes per policy", () => {
		const payload = buildCreatePolicyPayload({
			name: "COS Metadata",
			driver_type: "tencent_cos",
			endpoint: "https://cos.ap-guangzhou.myqcloud.com",
			bucket: "bucket-1250000000",
			access_key: "AKID",
			secret_key: "SECRET",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			object_storage_upload_strategy: "relay_stream",
			object_storage_download_strategy: "relay_stream",
			storage_native_processing_enabled: true,
			storage_native_media_metadata_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["jpg"],
			media_metadata_extensions: [
				" .MP4 ",
				"mp4",
				".Mov",
				"",
				"../../etc/passwd",
			],
		});

		expect(payload.options).toMatchObject({
			storage_native_media_metadata_enabled: true,
			media_metadata_extensions: ["mp4", "mov"],
		});
	});

	it("detects connection field changes per driver without treating policy options as connection changes", () => {
		const localPolicy = {
			id: 11,
			name: "Local",
			driver_type: "local",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "/srv/data",
			remote_node_id: null,
			max_file_size: null,
			allowed_types: [],
			options: {},
			is_default: false,
			chunk_size: 5 * 1024 * 1024,
			created_at: "",
			updated_at: "",
		} as StoragePolicy;
		const localForm = getPolicyForm(localPolicy);

		expect(
			buildCreatePolicyPayload({
				...localForm,
				content_dedup: true,
			}).options,
		).toEqual({ content_dedup: true });
		expect(hasConnectionFieldChanges(localForm, null)).toBe(true);
		expect(hasConnectionFieldChanges(localForm, localPolicy)).toBe(false);
		expect(
			hasConnectionFieldChanges(
				{ ...localForm, base_path: "/srv/other" },
				localPolicy,
			),
		).toBe(true);

		const remotePolicy = {
			...localPolicy,
			id: 12,
			name: "Remote",
			driver_type: "remote",
			base_path: "tenant-a",
			remote_node_id: 9,
			options: {
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
			},
		} as StoragePolicy;
		const remoteForm = getPolicyForm(remotePolicy);
		const remoteDescriptor = {
			capabilities: {
				remote_node_binding: true,
			},
			fields: [{ name: "remote_node_id", scope: "remote_node_binding" }],
			upload_workflows: {},
		} as never;

		expect(hasConnectionFieldChanges(remoteForm, remotePolicy)).toBe(false);
		expect(
			hasConnectionFieldChanges(
				{ ...remoteForm, remote_node_id: "10" },
				remotePolicy,
			),
		).toBe(false);
		expect(
			hasConnectionFieldChanges(
				{ ...remoteForm, remote_node_id: "10" },
				remotePolicy,
				remoteDescriptor,
			),
		).toBe(true);

		const s3Policy = {
			...localPolicy,
			id: 13,
			name: "S3",
			driver_type: "s3",
			endpoint: "https://s3.example.com",
			bucket: "media",
			base_path: "uploads",
			options: {
				object_storage_download_strategy: "presigned",
				object_storage_upload_strategy: "presigned",
			},
		} as StoragePolicy;
		const s3Form = getPolicyForm(s3Policy);
		const s3Descriptor = staticSecretConnectionDescriptor();

		expect(hasConnectionFieldChanges(s3Form, s3Policy, s3Descriptor)).toBe(
			false,
		);
		expect(
			hasConnectionFieldChanges(
				{ ...s3Form, secret_key: "SECRET" },
				s3Policy,
				s3Descriptor,
			),
		).toBe(true);

		const sftpPolicy = {
			...localPolicy,
			id: 16,
			name: "SFTP",
			driver_type: "sftp",
			endpoint: "sftp.example.com:22",
			access_key: "aster",
			secret_key: "",
			base_path: "uploads",
			options: {
				sftp_host_key_fingerprint: "SHA256:old-key",
			},
		} as StoragePolicy;
		const sftpForm = getPolicyForm(sftpPolicy);
		const sftpDescriptor = {
			credential_mode: "static_secret",
			fields: [
				{ name: "endpoint", scope: "connection" },
				{ name: "access_key", scope: "connection" },
				{ name: "secret_key", scope: "connection" },
				{
					name: "sftp_host_key_fingerprint",
					kind: "text",
					scope: "policy_options",
				},
			],
			upload_workflows: {},
		} as never;

		expect(
			hasConnectionFieldChanges(sftpForm, sftpPolicy, sftpDescriptor),
		).toBe(false);
		expect(
			hasConnectionFieldChanges(
				{
					...sftpForm,
					policy_option_values: {
						...sftpForm.policy_option_values,
						sftp_host_key_fingerprint: " SHA256:new-key ",
					},
				},
				sftpPolicy,
				sftpDescriptor,
			),
		).toBe(true);

		const oneDrivePolicy = {
			...localPolicy,
			id: 14,
			name: "OneDrive",
			driver_type: "one_drive",
			options: {
				onedrive_account_mode: "work_or_school",
				onedrive_cloud: "global",
				onedrive_tenant: "common",
			},
		} as StoragePolicy;
		const oneDriveForm = getPolicyForm(oneDrivePolicy);

		expect(hasConnectionFieldChanges(oneDriveForm, oneDrivePolicy)).toBe(false);
		expect(
			hasConnectionFieldChanges(
				{ ...oneDriveForm, onedrive_root_item_id: "folder-id" },
				oneDrivePolicy,
			),
		).toBe(true);
	});
});

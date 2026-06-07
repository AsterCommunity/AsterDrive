import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	adminConfigService,
	adminExternalAuthService,
	adminFileService,
	adminLockService,
	adminOverviewService,
	adminPolicyGroupService,
	adminPolicyService,
	adminRemoteNodeService,
	adminShareService,
	adminTaskService,
	adminUserService,
} from "@/services/adminService";

const mockState = vi.hoisted(() => ({
	delete: vi.fn(),
	get: vi.fn(),
	patch: vi.fn(),
	post: vi.fn(),
	put: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		delete: mockState.delete,
		get: mockState.get,
		patch: mockState.patch,
		post: mockState.post,
		put: mockState.put,
	},
}));

describe("adminService", () => {
	beforeEach(() => {
		mockState.delete.mockReset();
		mockState.get.mockReset();
		mockState.patch.mockReset();
		mockState.post.mockReset();
		mockState.put.mockReset();
	});

	it("builds list endpoints with optional query strings", () => {
		adminUserService.list({
			limit: 20,
			offset: 40,
			keyword: "alice",
			role: "admin" as never,
			status: "active" as never,
			sort_by: "username",
			sort_order: "asc",
		});
		adminPolicyService.list({ limit: 5, offset: 10, sort_by: "name" });
		adminRemoteNodeService.list({ limit: 7, offset: 14, sort_order: "desc" });
		adminPolicyGroupService.list({
			limit: 6,
			offset: 12,
			sort_by: "updated_at",
			sort_order: "asc",
		});
		adminShareService.list({ limit: 8, offset: 16, sort_by: "created_at" });
		adminLockService.list({ limit: 9, sort_by: "path" });
		adminConfigService.list({ offset: 3 });

		expect(mockState.get).toHaveBeenNthCalledWith(
			1,
			"/admin/users?limit=20&offset=40&keyword=alice&role=admin&status=active&sort_by=username&sort_order=asc",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			2,
			"/admin/policies?limit=5&offset=10&sort_by=name",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			3,
			"/admin/remote-nodes?limit=7&offset=14&sort_order=desc",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			4,
			"/admin/policy-groups?limit=6&offset=12&sort_by=updated_at&sort_order=asc",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			5,
			"/admin/shares?limit=8&offset=16&sort_by=created_at",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			6,
			"/admin/locks?limit=9&sort_by=path",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(7, "/admin/config?offset=3");
	});

	it("uses bare list endpoints when no query params are provided", () => {
		adminUserService.list();
		adminUserService.listInvitations();
		adminPolicyService.list();
		adminRemoteNodeService.list();
		adminPolicyGroupService.list();
		adminShareService.list();
		adminLockService.list();
		adminConfigService.list();

		expect(mockState.get).toHaveBeenNthCalledWith(1, "/admin/users");
		expect(mockState.get).toHaveBeenNthCalledWith(
			2,
			"/admin/users/invitations",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(3, "/admin/policies");
		expect(mockState.get).toHaveBeenNthCalledWith(4, "/admin/remote-nodes");
		expect(mockState.get).toHaveBeenNthCalledWith(5, "/admin/policy-groups");
		expect(mockState.get).toHaveBeenNthCalledWith(6, "/admin/shares");
		expect(mockState.get).toHaveBeenNthCalledWith(7, "/admin/locks");
		expect(mockState.get).toHaveBeenNthCalledWith(8, "/admin/config");
	});

	it("uses admin user invitation endpoints", () => {
		adminUserService.listInvitations({ limit: 10, offset: 20 });
		adminUserService.createInvitation({ email: "invitee@example.com" });
		adminUserService.revokeInvitation(42);

		expect(mockState.get).toHaveBeenCalledWith(
			"/admin/users/invitations?limit=10&offset=20",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			1,
			"/admin/users/invitations",
			{ email: "invitee@example.com" },
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			2,
			"/admin/users/invitations/42/revoke",
		);
	});

	it("tests external auth provider draft parameters without an id", () => {
		adminExternalAuthService.testParams({
			client_id: "client-id",
			issuer_url: "https://idp.example.com",
			provider_kind: "oidc" as never,
			scopes: "openid email profile",
		});

		expect(mockState.post).toHaveBeenCalledWith(
			"/admin/external-auth/providers/test",
			{
				client_id: "client-id",
				issuer_url: "https://idp.example.com",
				provider_kind: "oidc",
				scopes: "openid email profile",
			},
		);
	});

	it("loads all policy groups across multiple pages", async () => {
		mockState.get
			.mockResolvedValueOnce({
				items: [{ id: 1 }, { id: 2 }],
				limit: 2,
				offset: 0,
				total: 3,
			})
			.mockResolvedValueOnce({
				items: [{ id: 3 }],
				limit: 2,
				offset: 2,
				total: 3,
			});

		await expect(adminPolicyGroupService.listAll(2)).resolves.toEqual([
			{ id: 1 },
			{ id: 2 },
			{ id: 3 },
		]);
		expect(mockState.get).toHaveBeenNthCalledWith(
			1,
			"/admin/policy-groups?limit=2&offset=0",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			2,
			"/admin/policy-groups?limit=2&offset=2",
		);
	});

	it("fails when policy group pagination returns an empty page before total is reached", async () => {
		mockState.get
			.mockResolvedValueOnce({
				items: [{ id: 1 }, { id: 2 }],
				limit: 2,
				offset: 0,
				total: 3,
			})
			.mockResolvedValueOnce({
				items: [],
				limit: 2,
				offset: 2,
				total: 3,
			});

		await expect(adminPolicyGroupService.listAll(2)).rejects.toThrow(
			"incomplete pages from adminPolicyGroupService.list",
		);
	});

	it("fails when policy group pagination exceeds the safety cap", async () => {
		mockState.get.mockResolvedValue({
			items: [{ id: 1 }],
			limit: 100,
			offset: 0,
			total: 100,
		});

		await expect(adminPolicyGroupService.listAll(100)).rejects.toThrow(
			"pagination exceeded max iterations",
		);
	});

	it("builds admin task list endpoints", () => {
		adminTaskService.list({
			limit: 12,
			offset: 24,
			sort_by: "display_name",
			sort_order: "asc",
		});
		adminTaskService.list();

		expect(mockState.get).toHaveBeenNthCalledWith(
			1,
			"/admin/tasks?limit=12&offset=24&sort_by=display_name&sort_order=asc",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/admin/tasks");
	});

	it("uses the expected detail and mutation endpoints", () => {
		adminOverviewService.get({
			days: 30,
			timezone: "Asia/Shanghai",
			event_limit: 16,
		});
		adminUserService.get(5);
		adminUserService.create({
			username: "alice",
			email: "alice@example.com",
			password: "secret",
		});
		adminUserService.update(5, {
			storage_quota: 1024,
			policy_group_id: 7,
		});
		adminUserService.resetPassword(5, { password: "newsecret" });
		adminUserService.revokeSessions(5);
		adminUserService.resetMfa(5);
		adminUserService.delete(5);

		adminPolicyService.get(3);
		adminPolicyService.getCapacity(3);
		adminPolicyService.create({
			name: "Primary",
			driver_type: "s3" as never,
			bucket: "bucket-a",
		});
		adminPolicyService.update(3, { is_default: true });
		adminPolicyService.delete(3);
		adminPolicyService.testConnection(3);
		adminPolicyService.testParams({
			driver_type: "s3" as never,
			endpoint: "https://example.com",
		});
		adminRemoteNodeService.get(6);
		adminRemoteNodeService.create({
			name: "Remote A",
			base_url: "https://remote.example.com",
		});
		adminRemoteNodeService.update(6, {
			base_url: "https://remote-b.example.com",
		});
		adminRemoteNodeService.delete(6);
		adminRemoteNodeService.testConnection(6);
		adminRemoteNodeService.testParams({
			base_url: "https://remote.example.com",
			access_key: "REMOTE",
			secret_key: "SECRET",
		});
		adminRemoteNodeService.createEnrollmentCommand(6);
		adminExternalAuthService.listKinds();
		adminExternalAuthService.list({ limit: 20, offset: 0 });
		adminExternalAuthService.get(15);
		adminExternalAuthService.create({
			client_id: "client-id",
			display_name: "Example IDP",
			icon_url: "/static/external-auth/example.svg",
			issuer_url: "https://idp.example.com",
			provider_kind: "oidc" as never,
		});
		adminExternalAuthService.update(15, {
			display_name: "Example IDP",
			enabled: true,
			icon_url: null,
		});
		adminExternalAuthService.test(15);
		adminExternalAuthService.delete(15);
		adminPolicyGroupService.get(4);
		adminPolicyGroupService.create({
			name: "Default Group",
			items: [{ policy_id: 3, priority: 1 }],
		});
		adminPolicyGroupService.update(4, { is_default: true });
		adminPolicyGroupService.migrateAssignments(4, { target_group_id: 8 });
		adminPolicyGroupService.delete(4);

		adminShareService.delete(11);

		adminLockService.forceUnlock(12);
		adminLockService.cleanupExpired();

		adminConfigService.schema();
		adminConfigService.templateVariables();
		adminConfigService.get("mail.host");
		adminConfigService.set("mail.host", "smtp.example.com");
		adminConfigService.delete("mail.host");

		expect(mockState.get).toHaveBeenNthCalledWith(
			1,
			"/admin/overview?days=30&timezone=Asia%2FShanghai&event_limit=16",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/admin/users/5");
		expect(mockState.post).toHaveBeenNthCalledWith(1, "/admin/users", {
			username: "alice",
			email: "alice@example.com",
			password: "secret",
		});
		expect(mockState.patch).toHaveBeenNthCalledWith(1, "/admin/users/5", {
			storage_quota: 1024,
			policy_group_id: 7,
		});
		expect(mockState.put).toHaveBeenNthCalledWith(
			1,
			"/admin/users/5/password",
			{
				password: "newsecret",
			},
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			2,
			"/admin/users/5/sessions/revoke",
		);
		expect(mockState.delete).toHaveBeenNthCalledWith(1, "/admin/users/5/mfa");
		expect(mockState.delete).toHaveBeenNthCalledWith(2, "/admin/users/5");

		expect(mockState.get).toHaveBeenNthCalledWith(3, "/admin/policies/3");
		expect(mockState.post).toHaveBeenNthCalledWith(3, "/admin/policies", {
			name: "Primary",
			driver_type: "s3",
			bucket: "bucket-a",
		});
		expect(mockState.patch).toHaveBeenNthCalledWith(2, "/admin/policies/3", {
			is_default: true,
		});
		expect(mockState.delete).toHaveBeenNthCalledWith(3, "/admin/policies/3");
		expect(mockState.post).toHaveBeenNthCalledWith(4, "/admin/policies/3/test");
		expect(mockState.get).toHaveBeenNthCalledWith(
			4,
			"/admin/policies/3/capacity",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(5, "/admin/policies/test", {
			driver_type: "s3",
			endpoint: "https://example.com",
		});
		expect(mockState.get).toHaveBeenNthCalledWith(5, "/admin/remote-nodes/6");
		expect(mockState.post).toHaveBeenNthCalledWith(6, "/admin/remote-nodes", {
			name: "Remote A",
			base_url: "https://remote.example.com",
		});
		expect(mockState.patch).toHaveBeenNthCalledWith(
			3,
			"/admin/remote-nodes/6",
			{
				base_url: "https://remote-b.example.com",
			},
		);
		expect(mockState.delete).toHaveBeenNthCalledWith(
			4,
			"/admin/remote-nodes/6",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			7,
			"/admin/remote-nodes/6/test",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			8,
			"/admin/remote-nodes/test",
			{
				base_url: "https://remote.example.com",
				access_key: "REMOTE",
				secret_key: "SECRET",
			},
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			9,
			"/admin/remote-nodes/6/enrollment-token",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			6,
			"/admin/external-auth/provider-kinds",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			7,
			"/admin/external-auth/providers?limit=20&offset=0",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			8,
			"/admin/external-auth/providers/15",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			10,
			"/admin/external-auth/providers",
			{
				client_id: "client-id",
				display_name: "Example IDP",
				icon_url: "/static/external-auth/example.svg",
				issuer_url: "https://idp.example.com",
				provider_kind: "oidc",
			},
		);
		expect(mockState.patch).toHaveBeenNthCalledWith(
			4,
			"/admin/external-auth/providers/15",
			{
				display_name: "Example IDP",
				enabled: true,
				icon_url: null,
			},
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			11,
			"/admin/external-auth/providers/15/test",
		);
		expect(mockState.delete).toHaveBeenNthCalledWith(
			5,
			"/admin/external-auth/providers/15",
		);

		expect(mockState.get).toHaveBeenNthCalledWith(9, "/admin/policy-groups/4");
		expect(mockState.post).toHaveBeenNthCalledWith(12, "/admin/policy-groups", {
			name: "Default Group",
			items: [{ policy_id: 3, priority: 1 }],
		});
		expect(mockState.patch).toHaveBeenNthCalledWith(
			5,
			"/admin/policy-groups/4",
			{
				is_default: true,
			},
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			13,
			"/admin/policy-groups/4/migrate-assignments",
			{
				target_group_id: 8,
			},
		);
		expect(mockState.delete).toHaveBeenNthCalledWith(
			6,
			"/admin/policy-groups/4",
		);

		expect(mockState.delete).toHaveBeenNthCalledWith(7, "/admin/shares/11");
		expect(mockState.delete).toHaveBeenNthCalledWith(8, "/admin/locks/12");
		expect(mockState.delete).toHaveBeenNthCalledWith(9, "/admin/locks/expired");

		expect(mockState.get).toHaveBeenNthCalledWith(10, "/admin/config/schema");
		expect(mockState.get).toHaveBeenNthCalledWith(
			11,
			"/admin/config/template-variables",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(
			12,
			"/admin/config/mail.host",
		);
		expect(mockState.put).toHaveBeenCalledWith("/admin/config/mail.host", {
			value: "smtp.example.com",
		});
		expect(mockState.delete).toHaveBeenNthCalledWith(
			10,
			"/admin/config/mail.host",
		);
	});

	it("uses the expected managed ingress profile endpoints", () => {
		adminRemoteNodeService.listIngressProfiles(6);
		adminRemoteNodeService.createIngressProfile(6, {
			name: "Ingress A",
			driver_type: "local" as never,
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "tenant-a/incoming",
			max_file_size: 2048,
			is_default: true,
		});
		adminRemoteNodeService.updateIngressProfile(6, "igp_demo", {
			name: "Ingress B",
			is_default: false,
		});
		adminRemoteNodeService.deleteIngressProfile(6, "igp_demo");

		expect(mockState.get).toHaveBeenCalledWith(
			"/admin/remote-nodes/6/ingress-profiles",
		);
		expect(mockState.post).toHaveBeenCalledWith(
			"/admin/remote-nodes/6/ingress-profiles",
			{
				name: "Ingress A",
				driver_type: "local",
				endpoint: "",
				bucket: "",
				access_key: "",
				secret_key: "",
				base_path: "tenant-a/incoming",
				max_file_size: 2048,
				is_default: true,
			},
		);
		expect(mockState.patch).toHaveBeenCalledWith(
			"/admin/remote-nodes/6/ingress-profiles/igp_demo",
			{
				name: "Ingress B",
				is_default: false,
			},
		);
		expect(mockState.delete).toHaveBeenCalledWith(
			"/admin/remote-nodes/6/ingress-profiles/igp_demo",
		);
	});

	it("creates storage policy migration tasks", () => {
		adminPolicyService.createMigration({
			source_policy_id: 3,
			target_policy_id: 9,
			delete_source_after_success: false,
		});

		expect(mockState.post).toHaveBeenCalledWith("/admin/storage-migrations", {
			source_policy_id: 3,
			target_policy_id: 9,
			delete_source_after_success: false,
		});
	});

	it("creates blob maintenance tasks", () => {
		adminFileService.createBlobMaintenanceTask({
			action: "orphan_cleanup",
			blob_ids: [31, 32],
		});
		adminFileService.createBlobMaintenanceTask({
			action: "ref_count_reconcile",
		});

		expect(mockState.post).toHaveBeenCalledWith(
			"/admin/file-blobs/maintenance",
			{
				action: "orphan_cleanup",
				blob_ids: [31, 32],
			},
		);
		expect(mockState.post).toHaveBeenCalledWith(
			"/admin/file-blobs/maintenance",
			{
				action: "ref_count_reconcile",
			},
		);
	});

	it("checks and resumes storage policy migrations", () => {
		adminPolicyService.dryRunMigration({
			source_policy_id: 3,
			target_policy_id: 9,
			delete_source_after_success: false,
		});
		adminTaskService.resumeStoragePolicyMigration(42);

		expect(mockState.post).toHaveBeenNthCalledWith(
			1,
			"/admin/storage-migrations/dry-run",
			{
				source_policy_id: 3,
				target_policy_id: 9,
				delete_source_after_success: false,
			},
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			2,
			"/admin/storage-migrations/42/resume",
		);
	});

	it("omits null policy_group_id values from update user payloads", () => {
		adminUserService.update(5, {
			role: "admin" as never,
			policy_group_id: null,
		} as never);

		expect(mockState.patch).toHaveBeenCalledWith("/admin/users/5", {
			role: "admin",
		});
	});
});

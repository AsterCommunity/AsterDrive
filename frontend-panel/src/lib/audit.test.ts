import { describe, expect, it } from "vitest";
import {
	formatAuditAction,
	formatAuditDetail,
	formatAuditEntityType,
	formatAuditSummary,
	formatAuditTarget,
	formatAuditTargetType,
	getAuditActionBadgeClass,
} from "@/lib/audit";
import { createMockTWithInterpolation } from "@/test/i18n";
import type { AuditLogEntry } from "@/types/api";

describe("audit i18n formatting", () => {
	it("uses admin audit translations before falling back to raw keys", () => {
		const t = createMockTWithInterpolation({
			"admin:audit_action_file_delete": "Deleted file",
			"admin:audit_action_offline_download": "Created link import task",
			"admin:audit_action_follower_object_write": "Follower wrote object",
			"admin:audit_action_tag_attach": "Attached tag",
			"admin:audit_action_team_webdav_account_create":
				"Created team WebDAV account",
			"admin:audit_action_webdav_account_create": "Created WebDAV account",
			"admin:audit_entity_type_resource_lock": "Resource lock",
			"admin:audit_entity_type_tag": "Tag",
		});

		expect(formatAuditAction(t, "file_delete")).toBe("Deleted file");
		expect(formatAuditAction(t, "offline_download")).toBe(
			"Created link import task",
		);
		expect(formatAuditAction(t, "follower_object_write")).toBe(
			"Follower wrote object",
		);
		expect(formatAuditAction(t, "tag_attach")).toBe("Attached tag");
		expect(formatAuditAction(t, "webdav_account_create")).toBe(
			"Created WebDAV account",
		);
		expect(formatAuditAction(t, "team_webdav_account_create")).toBe(
			"Created team WebDAV account",
		);
		expect(formatAuditEntityType(t, "resource_lock")).toBe("Resource lock");
		expect(formatAuditEntityType(t, "tag")).toBe("Tag");
	});

	it("keeps legacy settings translations as a fallback for team audit entries", () => {
		const t = createMockTWithInterpolation({
			"settings:team_member_add": "Member added",
		});

		expect(formatAuditAction(t, "team_member_add")).toBe("Member added");
	});

	it("falls back to raw values when no translation exists", () => {
		const t = createMockTWithInterpolation();

		expect(formatAuditAction(t, "unknown_action")).toBe("unknown_action");
		expect(formatAuditEntityType(t, "unknown_entity")).toBe("unknown_entity");
		expect(formatAuditEntityType(t, null)).toBe("---");
	});

	it("maps common audit actions to the same badge palette used by admin pages", () => {
		expect(getAuditActionBadgeClass("file_delete")).toContain("border-red-200");
		expect(getAuditActionBadgeClass("file_upload")).toContain(
			"border-emerald-200",
		);
		expect(getAuditActionBadgeClass("share_create")).toContain(
			"border-sky-200",
		);
		expect(getAuditActionBadgeClass("tag_delete")).toContain("border-red-200");
		expect(getAuditActionBadgeClass("tag_attach")).toContain("border-sky-200");
		expect(getAuditActionBadgeClass("user_login")).toContain(
			"border-amber-200",
		);
		expect(getAuditActionBadgeClass("config_update")).toContain(
			"border-border",
		);
	});

	it("falls back to the neutral palette for uncategorized actions", () => {
		expect(getAuditActionBadgeClass("admin_create_user")).toContain(
			"border-border",
		);
		expect(getAuditActionBadgeClass("team_member_add")).toContain(
			"border-border",
		);
		expect(getAuditActionBadgeClass("unknown_action")).toContain(
			"border-border",
		);
	});

	it("prefers structured presentation messages when they are available", () => {
		const t = createMockTWithInterpolation({
			"admin:audit_entity_type_file": "File",
			"admin:audit_presentation_config_value_updated":
				"Value changed to {{value}}",
			"admin:audit_presentation_file": "{{name}} · File",
			"admin:audit_presentation_file_upload": "Uploaded via presentation",
		});
		const entry = {
			action: "file_upload",
			entity_name: "legacy.txt",
			entity_type: "folder",
			presentation: {
				detail: {
					code: "config_value_updated",
					params: { value: "enabled" },
				},
				summary: { code: "file_upload" },
				target: {
					code: "file",
					params: { name: "report.pdf" },
				},
			},
		} as AuditLogEntry;

		expect(formatAuditSummary(t, entry)).toBe("Uploaded via presentation");
		expect(formatAuditTarget(t, entry)).toBe("report.pdf · File");
		expect(formatAuditTargetType(t, entry)).toBe("File");
		expect(formatAuditDetail(t, entry)).toBe("Value changed to enabled");
	});

	it("formats presentation summary codes as actions before matching detail templates", () => {
		const t = createMockTWithInterpolation({
			"admin:audit_action_mail_delivery_failed": "Email delivery failed",
			"admin:audit_presentation_mail_delivery_failed":
				"{{template_code}} to {{to_address}} failed: {{error}}",
		});
		const entry = {
			action: "mail_delivery_failed",
			entity_name: "mail",
			entity_type: "mail",
			presentation: {
				detail: {
					code: "mail_delivery_failed",
					params: {
						error: "smtp unavailable",
						template_code: "smtp_test",
						to_address: "alice@example.com",
					},
				},
				summary: { code: "mail_delivery_failed" },
				target: {
					code: "mail",
					params: { name: "mail" },
				},
			},
		} as AuditLogEntry;

		expect(formatAuditSummary(t, entry)).toBe("Email delivery failed");
		expect(formatAuditDetail(t, entry)).toBe(
			"smtp_test to alice@example.com failed: smtp unavailable",
		);
	});

	it("falls back safely when presentation codes are unknown or missing", () => {
		const t = createMockTWithInterpolation({
			"admin:audit_action_file_delete": "Deleted file",
			"admin:audit_entity_type_file": "File",
		});
		const entry = {
			action: "file_delete",
			entity_name: null,
			entity_type: "file",
			presentation: {
				detail: { code: "unknown_detail" },
				summary: { code: "unknown_summary" },
				target: { code: "unknown_target" },
			},
		} as AuditLogEntry;

		expect(formatAuditSummary(t, entry)).toBe("Deleted file");
		expect(formatAuditTarget(t, entry)).toBe("File");
		expect(formatAuditTargetType(t, entry)).toBe("File");
		expect(formatAuditDetail(t, entry)).toBeUndefined();
	});

	it("ignores array params in presentation messages", () => {
		const t = createMockTWithInterpolation({
			"admin:audit_presentation_config_value_updated": "Value {{0}}",
		});
		const entry = {
			action: "file_delete",
			entity_name: null,
			entity_type: "file",
			presentation: {
				detail: {
					code: "config_value_updated",
					params: ["unexpected"],
				},
			},
		} as AuditLogEntry;

		expect(formatAuditDetail(t, entry)).toBe("Value {{0}}");
	});

	it("formats offline download presentation details from structured params", () => {
		const t = createMockTWithInterpolation({
			"admin:audit_presentation_offline_download_created":
				"Import from {{source}} to folder {{target_folder_id}}",
		});
		const entry = {
			action: "offline_download",
			presentation: {
				detail: {
					code: "offline_download_created",
					params: {
						source: "https://example.test/archive.zip",
						target_folder_id: 42,
					},
				},
			},
		} as AuditLogEntry;

		expect(formatAuditDetail(t, entry)).toBe(
			"Import from https://example.test/archive.zip to folder 42",
		);
	});
});

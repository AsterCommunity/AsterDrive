import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import {
	formatAuditAction,
	formatAuditEntityType,
	getAuditActionBadgeClass,
} from "@/lib/audit";

function createT(translations: Record<string, string> = {}) {
	return ((key: string, options?: Record<string, unknown>) => {
		const namespace = typeof options?.ns === "string" ? options.ns : "admin";
		const translated = translations[`${namespace}:${key}`] ?? translations[key];
		if (translated) {
			return translated;
		}
		return typeof options?.defaultValue === "string"
			? options.defaultValue
			: key;
	}) as unknown as TFunction;
}

describe("audit i18n formatting", () => {
	it("uses admin audit translations before falling back to raw keys", () => {
		const t = createT({
			"admin:audit_action_file_delete": "Deleted file",
			"admin:audit_entity_type_resource_lock": "Resource lock",
		});

		expect(formatAuditAction(t, "file_delete")).toBe("Deleted file");
		expect(formatAuditEntityType(t, "resource_lock")).toBe("Resource lock");
	});

	it("keeps legacy settings translations as a fallback for team audit entries", () => {
		const t = createT({
			"settings:team_member_add": "Member added",
		});

		expect(formatAuditAction(t, "team_member_add")).toBe("Member added");
	});

	it("falls back to raw values when no translation exists", () => {
		const t = createT();

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
});

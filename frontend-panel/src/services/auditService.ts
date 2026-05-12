import { withQuery } from "@/lib/queryParams";
import { api } from "@/services/http";
import type { AdminAuditLogSortBy, AdminSortParams } from "@/types/adminSort";
import type { AuditLogPage } from "@/types/api";

interface AuditLogQuery extends AdminSortParams<AdminAuditLogSortBy> {
	user_id?: number;
	action?: string;
	entity_type?: string;
	entity_id?: number;
	after?: string;
	before?: string;
	limit?: number;
	offset?: number;
}

export const auditService = {
	list: (params: AuditLogQuery = {}) => {
		const { limit, offset, sort_by, sort_order, ...filters } = params;

		return api.get<AuditLogPage>(
			withQuery("/admin/audit-logs", {
				limit,
				offset,
				sort_by,
				sort_order,
				...filters,
			}),
		);
	},
};

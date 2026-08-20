import { withQuery } from "@/lib/queryParams";
import { api, downloadFile } from "@/services/http";
import type { AuditLogListQuery, AuditLogPage } from "@/types/api";

export const auditService = {
	list: (params: AuditLogListQuery = {}) => {
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
	export: (params: Omit<AuditLogListQuery, "limit" | "offset"> = {}) =>
		downloadFile(
			withQuery("/admin/audit-logs/export", params),
			undefined,
			"asterdrive-audit.csv",
		),
};

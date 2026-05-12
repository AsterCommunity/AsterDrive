import { useEffect, useEffectEvent, useRef, useState } from "react";
import { handleApiError } from "@/hooks/useApiError";
import type { SortOrder } from "@/lib/pagination";
import { adminTeamService } from "@/services/adminService";
import type { AdminTeamMemberSortBy } from "@/types/adminSort";
import type {
	AdminTeamInfo,
	TeamAuditEntryInfo,
	TeamMemberInfo,
	TeamMemberRole,
	UserStatus,
} from "@/types/api";
import {
	ADMIN_TEAM_DETAIL_AUDIT_PAGE_SIZE,
	ADMIN_TEAM_DETAIL_MEMBER_PAGE_SIZE,
} from "./adminTeamDetailDialogState";

interface AdminTeamMemberFilters {
	keyword?: string;
	role?: TeamMemberRole;
	status?: UserStatus;
}

interface UseAdminTeamDetailDataArgs {
	auditOffset: number;
	memberFilters: AdminTeamMemberFilters;
	memberOffset: number;
	memberSortBy: AdminTeamMemberSortBy;
	memberSortOrder: SortOrder;
	open: boolean;
	teamId: number | null;
}

export function useAdminTeamDetailData({
	auditOffset,
	memberFilters,
	memberOffset,
	memberSortBy,
	memberSortOrder,
	open,
	teamId,
}: UseAdminTeamDetailDataArgs) {
	const [auditEntries, setAuditEntries] = useState<TeamAuditEntryInfo[]>([]);
	const [auditLoading, setAuditLoading] = useState(false);
	const [auditTotal, setAuditTotal] = useState(0);
	const [detailLoading, setDetailLoading] = useState(false);
	const [memberLoading, setMemberLoading] = useState(false);
	const [memberTotal, setMemberTotal] = useState(0);
	const [members, setMembers] = useState<TeamMemberInfo[]>([]);
	const [managerCount, setManagerCount] = useState(0);
	const [ownerCount, setOwnerCount] = useState(0);
	const [team, setTeam] = useState<AdminTeamInfo | null>(null);
	const auditRequestIdRef = useRef(0);
	const detailRequestIdRef = useRef(0);
	const memberRequestIdRef = useRef(0);

	const loadTeamDetail = useEffectEvent(async (nextTeamId: number) => {
		const requestId = ++detailRequestIdRef.current;
		setDetailLoading(true);
		try {
			const detail = await adminTeamService.get(nextTeamId);
			if (requestId !== detailRequestIdRef.current) {
				return;
			}
			setTeam(detail);
		} catch (error) {
			if (requestId !== detailRequestIdRef.current) {
				return;
			}
			setTeam(null);
			handleApiError(error);
		} finally {
			if (requestId === detailRequestIdRef.current) {
				setDetailLoading(false);
			}
		}
	});

	const loadMembers = useEffectEvent(
		async (
			nextTeamId: number,
			nextOffset = memberOffset,
			nextFilters: AdminTeamMemberFilters = memberFilters,
			nextSortBy: AdminTeamMemberSortBy = memberSortBy,
			nextSortOrder: SortOrder = memberSortOrder,
		) => {
			const requestId = ++memberRequestIdRef.current;
			setMemberLoading(true);
			try {
				const page = await adminTeamService.listMembers(nextTeamId, {
					keyword: nextFilters.keyword,
					role: nextFilters.role,
					status: nextFilters.status,
					limit: ADMIN_TEAM_DETAIL_MEMBER_PAGE_SIZE,
					offset: nextOffset,
					sort_by: nextSortBy,
					sort_order: nextSortOrder,
				});
				if (requestId !== memberRequestIdRef.current) {
					return;
				}
				setMembers(page.items);
				setMemberTotal(page.total);
				setOwnerCount(page.owner_count);
				setManagerCount(page.manager_count);
			} catch (error) {
				if (requestId !== memberRequestIdRef.current) {
					return;
				}
				setMembers([]);
				setMemberTotal(0);
				setOwnerCount(0);
				setManagerCount(0);
				handleApiError(error);
			} finally {
				if (requestId === memberRequestIdRef.current) {
					setMemberLoading(false);
				}
			}
		},
	);

	const loadAuditEntries = useEffectEvent(
		async (nextTeamId: number, nextOffset = auditOffset) => {
			const requestId = ++auditRequestIdRef.current;
			setAuditLoading(true);
			try {
				const page = await adminTeamService.listAuditLogs(nextTeamId, {
					limit: ADMIN_TEAM_DETAIL_AUDIT_PAGE_SIZE,
					offset: nextOffset,
				});
				if (requestId !== auditRequestIdRef.current) {
					return;
				}
				setAuditEntries(page.items);
				setAuditTotal(page.total);
			} catch (error) {
				if (requestId !== auditRequestIdRef.current) {
					return;
				}
				setAuditEntries([]);
				setAuditTotal(0);
				handleApiError(error);
			} finally {
				if (requestId === auditRequestIdRef.current) {
					setAuditLoading(false);
				}
			}
		},
	);

	useEffect(() => {
		if (!open || teamId == null) {
			detailRequestIdRef.current += 1;
			setDetailLoading(false);
			setTeam(null);
			return;
		}

		void loadTeamDetail(teamId);
	}, [open, teamId]);

	useEffect(() => {
		if (!open || teamId == null) {
			auditRequestIdRef.current += 1;
			setAuditEntries([]);
			setAuditLoading(false);
			setAuditTotal(0);
			return;
		}

		void loadAuditEntries(teamId, auditOffset);
	}, [auditOffset, open, teamId]);

	useEffect(() => {
		if (!open || teamId == null) {
			memberRequestIdRef.current += 1;
			setMemberLoading(false);
			setMemberTotal(0);
			setMembers([]);
			setManagerCount(0);
			setOwnerCount(0);
			return;
		}

		void loadMembers(
			teamId,
			memberOffset,
			memberFilters,
			memberSortBy,
			memberSortOrder,
		);
	}, [
		memberFilters,
		memberOffset,
		memberSortBy,
		memberSortOrder,
		open,
		teamId,
	]);

	return {
		auditEntries,
		auditLoading,
		auditTotal,
		detailLoading,
		loadAuditEntries,
		loadMembers,
		loadTeamDetail,
		managerCount,
		memberLoading,
		memberTotal,
		members,
		ownerCount,
		team,
	};
}

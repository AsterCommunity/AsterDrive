import { useMemo, useState } from "react";
import { AdminTableList } from "@/components/common/AdminTableList";
import { Icon } from "@/components/ui/icon";
import type { WebdavAccountInfo } from "@/types/api";
import { WebdavAccountHeaderRow } from "./WebdavAccountHeaderRow";
import { WebdavAccountRow } from "./WebdavAccountRow";

interface WebdavAccountTableLabels {
	accessScope: string;
	actions: string;
	active: string;
	allFiles: string;
	cancel: string;
	createdAt: string;
	delete: string;
	deleting: string;
	disabled: string;
	emptyDescription: string;
	emptyTitle: string;
	owner?: string;
	status: string;
	toggleUpdating: string;
	username: string;
}

interface WebdavAccountTableProps {
	accounts: WebdavAccountInfo[];
	canManageTeam?: boolean;
	currentUserId?: number | null;
	deletingAccountId: number | null;
	/** D9 用户页去框化：空态/工具条不带 AdminSurface 框 */
	frameless?: boolean;
	labels: WebdavAccountTableLabels;
	loading: boolean;
	onDelete: (id: number) => void;
	onToggle: (id: number) => void;
	togglingAccountId: number | null;
}

export function WebdavAccountTable({
	accounts,
	canManageTeam = false,
	currentUserId,
	deletingAccountId,
	frameless = false,
	labels,
	loading,
	onDelete,
	onToggle,
	togglingAccountId,
}: WebdavAccountTableProps) {
	const [pendingDeleteId, setPendingDeleteId] = useState<number | null>(null);
	const accountIds = useMemo(
		() => new Set(accounts.map((account) => account.id)),
		[accounts],
	);
	const headerRow = useMemo(
		() => (
			<WebdavAccountHeaderRow
				usernameLabel={labels.username}
				ownerLabel={canManageTeam ? labels.owner : undefined}
				scopeLabel={labels.accessScope}
				statusLabel={labels.status}
				createdAtLabel={labels.createdAt}
				actionLabel={labels.actions}
			/>
		),
		[canManageTeam, labels],
	);

	const activePendingDeleteId =
		pendingDeleteId != null && accountIds.has(pendingDeleteId)
			? pendingDeleteId
			: null;

	return (
		<AdminTableList
			loading={loading}
			items={accounts}
			columns={canManageTeam ? 6 : 5}
			rows={5}
			frameless={frameless}
			emptyIcon={<Icon name="Globe" className="size-10" />}
			emptyTitle={labels.emptyTitle}
			emptyDescription={labels.emptyDescription}
			headerRow={headerRow}
			renderRow={(account) => {
				const deleting = deletingAccountId === account.id;
				const toggling = togglingAccountId === account.id;
				const canMutate = canManageTeam || account.user_id === currentUserId;
				const confirmingDelete = activePendingDeleteId === account.id;
				const deleteLabel = deleting ? labels.deleting : labels.delete;
				const toggleLabel = toggling
					? labels.toggleUpdating
					: account.is_active
						? labels.disabled
						: labels.active;

				return (
					<WebdavAccountRow
						key={account.id}
						account={account}
						activeLabel={labels.active}
						allFilesLabel={labels.allFiles}
						canShowOwner={canManageTeam}
						canMutate={canMutate}
						cancelLabel={labels.cancel}
						deleteLabel={deleteLabel}
						disabledLabel={labels.disabled}
						deleting={deleting}
						confirmingDelete={confirmingDelete}
						toggling={toggling}
						onCancelDelete={() => setPendingDeleteId(null)}
						onConfirmDelete={() => {
							setPendingDeleteId(null);
							onDelete(account.id);
						}}
						onRequestDelete={() => setPendingDeleteId(account.id)}
						onToggle={onToggle}
						toggleLabel={toggleLabel}
					/>
				);
			}}
		/>
	);
}

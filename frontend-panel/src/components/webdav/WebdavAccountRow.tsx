import { UserIdentity } from "@/components/common/UserIdentity";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { TableCell, TableRow } from "@/components/ui/table";
import { formatDateShort } from "@/lib/format";
import type { WebdavAccessMode, WebdavAccountInfo } from "@/types/api";

interface WebdavAccountRowProps {
	account: WebdavAccountInfo;
	activeLabel: string;
	allFilesLabel: string;
	canShowOwner?: boolean;
	canMutate?: boolean;
	cancelLabel: string;
	confirmingDelete: boolean;
	deleteLabel: string;
	disabledLabel: string;
	deleting: boolean;
	readOnlyLabel: string;
	readWriteLabel: string;
	setAccessModePending: boolean;
	setAccessModePendingHere: boolean;
	setReadOnlyLabel: string;
	setReadWriteLabel: string;
	toggling: boolean;
	onCancelDelete: () => void;
	onConfirmDelete: () => void;
	onRequestDelete: () => void;
	onSetAccessMode: (id: number, accessMode: WebdavAccessMode) => void;
	onToggle: (id: number) => void;
	toggleLabel: string;
}

export function WebdavAccountRow({
	account,
	activeLabel,
	allFilesLabel,
	canShowOwner = false,
	canMutate = true,
	cancelLabel,
	confirmingDelete,
	deleteLabel,
	disabledLabel,
	deleting,
	readOnlyLabel,
	readWriteLabel,
	setAccessModePending,
	setAccessModePendingHere,
	setReadOnlyLabel,
	setReadWriteLabel,
	toggling,
	onCancelDelete,
	onConfirmDelete,
	onRequestDelete,
	onSetAccessMode,
	onToggle,
	toggleLabel,
}: WebdavAccountRowProps) {
	const readOnly = account.access_mode === "read_only";
	const setAccessModeLabel = readOnly ? setReadWriteLabel : setReadOnlyLabel;

	return (
		<TableRow key={account.id}>
			<TableCell>
				<div className="min-w-[140px]">
					<span className="truncate font-mono text-sm font-medium text-foreground">
						{account.username}
					</span>
				</div>
			</TableCell>
			{canShowOwner ? (
				<TableCell>
					<UserIdentity
						user={account.user}
						fallbackLabel={`#${account.user_id}`}
					/>
				</TableCell>
			) : null}
			<TableCell>
				<div className="flex min-w-[180px] items-center gap-2 text-sm text-foreground">
					<Icon
						name={account.root_folder_path ? "FolderOpen" : "Globe"}
						className="size-3.5 shrink-0 text-muted-foreground"
					/>
					<span className="truncate">
						{account.root_folder_path ?? allFilesLabel}
					</span>
				</div>
			</TableCell>
			<TableCell>
				<Badge
					variant={readOnly ? "outline" : "secondary"}
					className={
						readOnly
							? "bg-amber-500/10 text-amber-700 dark:text-amber-300"
							: undefined
					}
				>
					<Icon
						name={readOnly ? "Eye" : "PencilSimple"}
						className="mr-1 size-3"
					/>
					{readOnly ? readOnlyLabel : readWriteLabel}
				</Badge>
			</TableCell>
			<TableCell>
				<Badge
					variant={account.is_active ? "secondary" : "outline"}
					className={
						account.is_active
							? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
							: undefined
					}
				>
					{account.is_active ? activeLabel : disabledLabel}
				</Badge>
			</TableCell>
			<TableCell className="text-sm text-muted-foreground">
				{formatDateShort(account.created_at)}
			</TableCell>
			<TableCell>
				<div className="flex justify-end gap-2">
					<Button
						type="button"
						variant="outline"
						size="icon-sm"
						onClick={() =>
							onSetAccessMode(account.id, readOnly ? "read_write" : "read_only")
						}
						title={setAccessModeLabel}
						aria-label={setAccessModeLabel}
						disabled={
							!canMutate || setAccessModePending || deleting || toggling
						}
					>
						<Icon
							name={setAccessModePendingHere ? "Spinner" : "Eye"}
							className={`size-3.5 ${setAccessModePendingHere ? "animate-spin" : ""}`}
						/>
					</Button>
					<Button
						type="button"
						variant="outline"
						size="icon-sm"
						onClick={() => onToggle(account.id)}
						title={toggleLabel}
						aria-label={toggleLabel}
						disabled={
							!canMutate || toggling || deleting || setAccessModePending
						}
					>
						<Icon
							name={toggling ? "Spinner" : "Power"}
							className={`size-3.5 ${toggling ? "animate-spin" : ""}`}
						/>
					</Button>
					{confirmingDelete ? (
						<div className="flex items-center gap-2 duration-150 animate-in fade-in zoom-in-95 motion-reduce:animate-none">
							<Button
								type="button"
								variant="destructive"
								size="sm"
								onClick={onConfirmDelete}
								disabled={!canMutate || deleting || toggling}
							>
								{deleteLabel}
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={onCancelDelete}
								disabled={deleting || toggling}
							>
								{cancelLabel}
							</Button>
						</div>
					) : (
						<Button
							type="button"
							variant="destructive"
							size="icon-sm"
							onClick={onRequestDelete}
							title={deleteLabel}
							aria-label={deleteLabel}
							disabled={!canMutate || deleting || toggling}
						>
							<Icon
								name={deleting ? "Spinner" : "Trash"}
								className={`size-3.5 ${deleting ? "animate-spin" : ""}`}
							/>
						</Button>
					)}
				</div>
			</TableCell>
		</TableRow>
	);
}

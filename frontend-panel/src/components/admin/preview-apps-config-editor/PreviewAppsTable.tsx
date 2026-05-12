import { useTranslation } from "react-i18next";
import {
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { PreviewAppIcon } from "@/components/common/PreviewAppIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import {
	isProtectedBuiltinPreviewAppKey,
	type PreviewAppsEditorApp,
	type PreviewAppsEditorConfig,
} from "../previewAppsConfigEditorShared";
import {
	getAppHeading,
	getAppSummary,
	getProviderDefaultIcon,
	isInternalPreviewApp,
} from "./previewAppsConfigEditorPresentation";

interface PreviewAppsTableProps {
	appRowKeys: string[];
	draft: PreviewAppsEditorConfig;
	editingAppIndex: number | null;
	language: string | undefined;
	onDeleteApp: (index: number) => void;
	onEditApp: (index: number) => void;
	onMoveApp: (index: number, direction: -1 | 1) => void;
	onUpdateApp: (
		index: number,
		updater: (app: PreviewAppsEditorApp) => PreviewAppsEditorApp,
	) => void;
}

export function PreviewAppsTable({
	appRowKeys,
	draft,
	editingAppIndex,
	language,
	onDeleteApp,
	onEditApp,
	onMoveApp,
	onUpdateApp,
}: PreviewAppsTableProps) {
	const { t } = useTranslation(["admin", "files"]);

	if (draft.apps.length === 0) {
		return (
			<p className="text-sm text-muted-foreground">
				{t("preview_apps_no_apps")}
			</p>
		);
	}

	return (
		<div className="overflow-hidden rounded-lg border border-border/60 bg-background">
			<Table>
				<TableHeader>
					<TableRow>
						<TableHead className="w-16">
							{t("preview_apps_icon_label")}
						</TableHead>
						<TableHead>{t("preview_apps_column_app")}</TableHead>
						<TableHead>{t("preview_apps_column_summary")}</TableHead>
						<TableHead className="w-24">{t("preview_apps_enabled")}</TableHead>
						<TableHead className="w-48">{t("core:actions")}</TableHead>
					</TableRow>
				</TableHeader>
				<TableBody>
					{draft.apps.map((app, index) => {
						const rowEditing = editingAppIndex === index;
						const rowKey = appRowKeys[index] ?? app.key;
						const internalApp = isInternalPreviewApp(app);
						const protectedBuiltin = isProtectedBuiltinPreviewAppKey(app.key);
						const appHeading = getAppHeading(app, index, language, t);

						return (
							<TableRow
								key={rowKey}
								className={cn(rowEditing ? "bg-muted/20" : "")}
							>
								<TableCell>
									<div className="flex size-9 items-center justify-center rounded-xl border border-border/50 bg-muted/25">
										<PreviewAppIcon
											icon={app.icon}
											fallback={getProviderDefaultIcon(app.key, app.provider)}
											className="h-4 w-4"
										/>
									</div>
								</TableCell>
								<TableCell className="whitespace-normal">
									<div className="space-y-1">
										<div className="flex flex-wrap items-center gap-2">
											<span className="font-medium">{appHeading}</span>
											<Badge variant="outline">
												{internalApp
													? t("preview_apps_internal_badge")
													: t("preview_apps_external_badge")}
											</Badge>
										</div>
									</div>
								</TableCell>
								<TableCell className="whitespace-normal">
									<p className="line-clamp-2 text-sm text-muted-foreground break-all">
										{getAppSummary(app, t)}
									</p>
								</TableCell>
								<TableCell>
									<div className="flex items-center gap-2">
										<Switch
											size="sm"
											checked={app.enabled}
											onCheckedChange={(enabled) =>
												onUpdateApp(index, (current) => ({
													...current,
													enabled,
												}))
											}
										/>
										<span className="text-xs text-muted-foreground">
											{app.enabled
												? t("preview_apps_enabled")
												: t("preview_apps_disabled")}
										</span>
									</div>
								</TableCell>
								<TableCell>
									<div className="flex items-center justify-end gap-1">
										<Button
											variant="ghost"
											size="icon-sm"
											disabled={index === 0}
											aria-label={t("preview_apps_move_up")}
											onClick={() => onMoveApp(index, -1)}
										>
											<Icon name="ArrowUp" className="h-4 w-4" />
										</Button>
										<Button
											variant="ghost"
											size="icon-sm"
											disabled={index === draft.apps.length - 1}
											aria-label={t("preview_apps_move_down")}
											onClick={() => onMoveApp(index, 1)}
										>
											<Icon name="ArrowDown" className="h-4 w-4" />
										</Button>
										<Button
											variant="ghost"
											size="icon-sm"
											aria-label={t("preview_apps_edit")}
											onClick={() => onEditApp(index)}
										>
											<Icon name="PencilSimple" className="h-4 w-4" />
										</Button>
										<Button
											variant="ghost"
											size="icon-sm"
											className="text-destructive"
											disabled={protectedBuiltin}
											aria-label={
												protectedBuiltin
													? t("preview_apps_builtin_delete_disabled")
													: t("core:delete")
											}
											onClick={() => {
												if (!protectedBuiltin) {
													onDeleteApp(index);
												}
											}}
										>
											<Icon name="Trash" className="h-4 w-4" />
										</Button>
									</div>
								</TableCell>
							</TableRow>
						);
					})}
				</TableBody>
			</Table>
		</div>
	);
}

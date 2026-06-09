import { useTranslation } from "react-i18next";
import { AdminFilterToolbar } from "@/components/common/AdminFilterToolbar";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";

interface AdminTeamsToolbarProps {
	activeFilterCount: number;
	keyword: string;
	onArchivedToggle: () => void;
	onKeywordChange: (value: string) => void;
	onResetFilters: () => void;
	showArchived: boolean;
}

export function AdminTeamsToolbar({
	activeFilterCount,
	keyword,
	onArchivedToggle,
	onKeywordChange,
	onResetFilters,
	showArchived,
}: AdminTeamsToolbarProps) {
	const { t } = useTranslation("admin");

	return (
		<AdminFilterToolbar
			activeFilterCount={activeFilterCount}
			inline
			onResetFilters={onResetFilters}
		>
			<div className="relative min-w-[240px] flex-1 md:max-w-sm">
				<Icon
					name="MagnifyingGlass"
					className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					value={keyword}
					onChange={(event) => onKeywordChange(event.target.value)}
					placeholder={t("team_search_placeholder")}
					className={`${ADMIN_CONTROL_HEIGHT_CLASS} pl-9`}
				/>
			</div>
			<Button
				variant={showArchived ? "default" : "outline"}
				size="sm"
				className={ADMIN_CONTROL_HEIGHT_CLASS}
				onClick={onArchivedToggle}
			>
				<Icon name="Cloud" className="mr-1 size-4" />
				{showArchived ? t("show_active_teams") : t("show_archived_teams")}
			</Button>
		</AdminFilterToolbar>
	);
}

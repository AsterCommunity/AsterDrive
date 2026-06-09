import type { ReactNode } from "react";
import { useAdminSettingsCategoryContent } from "@/components/admin/settings/AdminSettingsCategoryContentContext";
import { ADMIN_SETTINGS_CONTENT_MAX_WIDTH_CLASS } from "@/components/admin/settings/adminSettingsAnimation";
import { cn } from "@/lib/utils";

export function AdminSettingsCategoryHeader(props: {
	category: string;
	description?: string;
	extra?: ReactNode;
}) {
	const { getCategoryDescription, getCategoryLabel } =
		useAdminSettingsCategoryContent();
	const resolvedDescription = Object.hasOwn(props, "description")
		? props.description
		: getCategoryDescription(props.category);

	return (
		<div className={cn(ADMIN_SETTINGS_CONTENT_MAX_WIDTH_CLASS, "space-y-2")}>
			<div className="space-y-1">
				<h3 className="text-lg font-semibold tracking-tight">
					{getCategoryLabel(props.category)}
				</h3>
				{resolvedDescription ? (
					<p className="max-w-3xl break-words text-sm leading-5 text-muted-foreground">
						{resolvedDescription}
					</p>
				) : null}
			</div>
			{props.extra}
		</div>
	);
}

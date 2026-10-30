import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";
import { Icon, type IconName } from "@/components/ui/icon";
import { SIDEBAR_SECTION_TITLE_CLASS, sidebarNavItemClass } from "@/lib/utils";
import type { SidebarContentProps } from "./sidebarTypes";

type SidebarNavigationItem = {
	key: string;
	icon: IconName;
	label: string;
	active: boolean;
} & (
	| {
			to: string;
			onClick?: () => void;
			dragOver?: boolean;
			onDragOver?: SidebarContentProps["onTrashDragOver"];
			onDragLeave?: SidebarContentProps["onTrashDragLeave"];
			onDrop?: SidebarContentProps["onTrashDropEvent"];
	  }
	| { to?: never; onClick: () => void }
);

export function SidebarNavigation({
	items,
}: {
	items: SidebarNavigationItem[];
}) {
	const { t } = useTranslation();
	return (
		<div className="p-2 space-y-1">
			<p className={SIDEBAR_SECTION_TITLE_CLASS}>{t("core:nav_section")}</p>
			{items.map((item) => {
				const className = sidebarNavItemClass(
					item.active,
					[
						"w-full text-left",
						"dragOver" in item &&
							item.dragOver &&
							"bg-destructive/10 text-destructive ring-1 ring-destructive/30",
					],
					{ indicator: true },
				);
				const content = (
					<>
						<Icon name={item.icon} className="size-4 shrink-0" />
						{item.label}
					</>
				);
				return item.to != null ? (
					<Link
						key={item.key}
						to={item.to}
						onClick={item.onClick}
						onDragOver={item.onDragOver}
						onDragLeave={item.onDragLeave}
						onDrop={item.onDrop}
						className={className}
						aria-current={item.active ? "page" : undefined}
					>
						{content}
					</Link>
				) : (
					<button
						key={item.key}
						type="button"
						onClick={item.onClick}
						className={className}
						aria-current={item.active ? "page" : undefined}
					>
						{content}
					</button>
				);
			})}
		</div>
	);
}

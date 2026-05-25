import { Link } from "react-router-dom";
import { Icon } from "@/components/ui/icon";
import { sidebarNavItemClass } from "@/lib/utils";
import type { SidebarContentProps } from "./sidebarTypes";

type SidebarNavigationProps = Pick<
	SidebarContentProps,
	| "locationPathname"
	| "navLinks"
	| "onMobileClose"
	| "onTrashDragLeave"
	| "onTrashDragOver"
	| "onTrashDropEvent"
	| "trashDragOver"
	| "trashPath"
>;

export function SidebarNavigation({
	locationPathname,
	navLinks,
	onMobileClose,
	onTrashDragLeave,
	onTrashDragOver,
	onTrashDropEvent,
	trashDragOver,
	trashPath,
}: SidebarNavigationProps) {
	return (
		<div className="p-2 space-y-1">
			{navLinks.map((link) => (
				<Link
					key={link.to}
					to={link.to}
					onClick={onMobileClose}
					onDragOver={link.to === trashPath ? onTrashDragOver : undefined}
					onDragLeave={link.to === trashPath ? onTrashDragLeave : undefined}
					onDrop={link.to === trashPath ? onTrashDropEvent : undefined}
					className={sidebarNavItemClass(
						locationPathname === link.to,
						link.to === trashPath &&
							trashDragOver &&
							"bg-destructive/10 text-destructive ring-1 ring-destructive/30",
					)}
				>
					<Icon name={link.icon} className="size-4 shrink-0" />
					{link.label}
				</Link>
			))}
		</div>
	);
}

import { type ReactNode, useSyncExternalStore } from "react";
import { PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";

interface AdminPageHeaderProps {
	title: string;
	description?: string;
	actions?: ReactNode;
	toolbar?: ReactNode;
	className?: string;
}

const ADMIN_HEADER_MOBILE_QUERY = "(max-width: 767px)";

function getAdminHeaderMobileSnapshot() {
	if (typeof window.matchMedia !== "function") {
		return false;
	}

	return window.matchMedia(ADMIN_HEADER_MOBILE_QUERY).matches;
}

function subscribeAdminHeaderMobileLayout(onStoreChange: () => void) {
	if (typeof window.matchMedia !== "function") {
		return () => {};
	}

	const mediaQuery = window.matchMedia(ADMIN_HEADER_MOBILE_QUERY);
	mediaQuery.addEventListener("change", onStoreChange);
	return () => {
		mediaQuery.removeEventListener("change", onStoreChange);
	};
}

function useAdminHeaderMobileLayout() {
	return useSyncExternalStore(
		subscribeAdminHeaderMobileLayout,
		getAdminHeaderMobileSnapshot,
		() => false,
	);
}

export function AdminPageHeader({
	title,
	description,
	actions,
	toolbar,
	className,
}: AdminPageHeaderProps) {
	const isMobile = useAdminHeaderMobileLayout();
	const hasMobileControls = actions || toolbar;

	return (
		<div
			className={cn(
				"space-y-4 border-b pb-4",
				PAGE_SECTION_PADDING_CLASS,
				className,
			)}
		>
			<div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
				<div className="space-y-1">
					<h2 className="text-xl font-semibold tracking-tight">{title}</h2>
					{description ? (
						<p className="max-w-3xl text-sm text-muted-foreground">
							{description}
						</p>
					) : null}
				</div>
				{actions && !isMobile ? (
					<div className="flex flex-wrap items-center gap-2">{actions}</div>
				) : null}
			</div>
			{isMobile && hasMobileControls ? (
				<div className="flex flex-wrap items-center gap-2">
					{actions}
					{toolbar}
				</div>
			) : null}
			{!isMobile && toolbar ? (
				<div className="flex flex-wrap items-center gap-2">{toolbar}</div>
			) : null}
		</div>
	);
}

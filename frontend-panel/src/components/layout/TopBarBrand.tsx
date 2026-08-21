import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { cn } from "@/lib/utils";

interface TopBarBrandProps {
	mobileVisible?: boolean;
	to?: string;
	ariaLabel?: string;
}

export function TopBarBrand({
	mobileVisible = false,
	to = "/",
	ariaLabel,
}: TopBarBrandProps) {
	const { t } = useTranslation(["core", "auth"]);
	const visibilityClassName = mobileVisible ? "block" : "hidden md:block";

	return (
		<div className="flex min-w-0 items-center gap-2 sm:gap-3">
			<Link
				to={to}
				aria-label={ariaLabel ?? t("auth:go_home")}
				className={cn(
					"shrink-0 rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
					visibilityClassName,
				)}
			>
				<AsterDriveWordmark
					alt={t("app_name")}
					className="h-16 w-auto shrink-0 px-6"
				/>
			</Link>
		</div>
	);
}

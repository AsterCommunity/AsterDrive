import { useTranslation } from "react-i18next";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { cn } from "@/lib/utils";

export function TopBarBrand({
	mobileVisible = false,
}: {
	mobileVisible?: boolean;
}) {
	const { t } = useTranslation();

	return (
		<div className="flex min-w-0 items-center gap-2 sm:gap-3">
			<AsterDriveWordmark
				alt={t("app_name")}
				className={cn(
					"h-16 w-auto shrink-0 px-6",
					mobileVisible ? "block" : "hidden md:block",
				)}
			/>
		</div>
	);
}

import { useTranslation } from "react-i18next";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { TopBarShell } from "@/components/layout/TopBarShell";

export function ShareTopBar() {
	const { t } = useTranslation();

	return (
		<TopBarShell
			heightClassName="h-14"
			left={
				<AsterDriveWordmark
					alt={t("app_name")}
					className="h-11 w-auto shrink-0 sm:h-12"
				/>
			}
			right={<span className="sr-only">{t("files:share")}</span>}
		/>
	);
}

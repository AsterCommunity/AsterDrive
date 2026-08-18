import { useTranslation } from "react-i18next";
import { useShallow } from "zustand/react/shallow";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { DEFAULT_BRANDING } from "@/lib/branding";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";

/**
 * 品牌区标语：管理员自定义过站点描述就展示实例自己的声音，
 * 否则回落到 i18n 产品标语（默认描述是英文，对中文界面不友好）。
 */
export function useAuthSlogan(): string {
	const { t } = useTranslation("login");
	const description = useFrontendConfigStore((s) => s.branding.description);
	return description !== DEFAULT_BRANDING.description
		? description
		: t("slogan");
}

export function AuthBrandPanel() {
	const { faviconUrl, title } = useFrontendConfigStore(
		useShallow((s) => ({
			faviconUrl: s.branding.faviconUrl,
			title: s.branding.title,
		})),
	);
	const slogan = useAuthSlogan();

	return (
		<div className="hidden items-center justify-center bg-sidebar lg:flex lg:w-1/2">
			<div className="login-enter flex max-w-md flex-col items-center px-12 text-center">
				<img
					src={faviconUrl}
					alt=""
					draggable={false}
					className="size-20 select-none"
				/>
				<AsterDriveWordmark alt={title} className="mt-8 h-28 w-auto" />
				<p className="mt-4 text-sm leading-relaxed text-muted-foreground">
					{slogan}
				</p>
				<p className="mt-2 text-xs text-muted-foreground/60">
					{window.location.host}
				</p>
			</div>
		</div>
	);
}

import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/common/EmptyState";
import { Icon } from "@/components/ui/icon";
import { PreviewSurface, PreviewSurfaceContent } from "./PreviewSurface";

export function PreviewUnavailable() {
	const { t } = useTranslation("files");

	return (
		<PreviewSurface>
			<PreviewSurfaceContent>
				<EmptyState
					icon={<Icon name="EyeSlash" className="size-10" />}
					title={t("preview_not_available")}
					description={t("preview_not_available_desc")}
				/>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}

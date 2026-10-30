import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { handleApiError } from "@/hooks/useApiError";
import { startAdminContentDownload } from "@/services/adminContentDownload";
import type { AdminFileDetail } from "@/types/api";

export function AdminFileDownloads({
	file,
	revisionId,
}: {
	file: AdminFileDetail;
	revisionId?: number;
}) {
	const { t } = useTranslation("admin");
	const [busy, setBusy] = useState(false);
	const pending = useRef(false);
	const selected =
		revisionId == null
			? null
			: file.versions.find((version) => version.id === revisionId);
	if (revisionId != null && !selected)
		return <p role="alert">{t("content_revision_unavailable")}</p>;
	if (file.team_id == null && file.owner_user_id == null)
		return <p role="alert">{t("content_access_failed")}</p>;
	const download = async () => {
		if (pending.current) return;
		pending.current = true;
		setBusy(true);
		try {
			await startAdminContentDownload(undefined, file.id, revisionId);
		} catch (error) {
			handleApiError(error);
		} finally {
			pending.current = false;
			setBusy(false);
		}
	};
	return (
		<Button disabled={busy} onClick={() => void download()}>
			{selected
				? t("content_download_revision", { version: selected.version })
				: t("content_download_current")}
		</Button>
	);
}

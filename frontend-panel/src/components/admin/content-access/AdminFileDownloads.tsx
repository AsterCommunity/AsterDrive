import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { handleApiError } from "@/hooks/useApiError";
import { startAdminContentDownload } from "@/services/adminContentDownload";
import type { AdminFileDetail } from "@/types/api";
import { AdminContentAccessGate } from "./AdminContentAccessGate";

function Downloads({
	accessId,
	file,
	revisionId,
}: {
	accessId: string;
	file: AdminFileDetail;
	revisionId?: number;
}) {
	const { t } = useTranslation("admin");
	const [busy, setBusy] = useState(false);
	const selected =
		revisionId == null
			? null
			: file.versions.find((version) => version.id === revisionId);
	const download = async () => {
		setBusy(true);
		try {
			await startAdminContentDownload(accessId, file.id, revisionId);
		} catch (error) {
			handleApiError(error);
		} finally {
			setBusy(false);
		}
	};
	if (revisionId != null && !selected)
		return <p role="alert">{t("content_revision_unavailable")}</p>;
	return (
		<Button disabled={busy} onClick={() => void download()}>
			{selected
				? t("content_download_revision", { version: selected.version })
				: t("content_download_current")}
		</Button>
	);
}

export function AdminFileDownloads({
	file,
	revisionId,
}: {
	file: AdminFileDetail;
	revisionId?: number;
}) {
	const { t } = useTranslation("admin");
	const [active, setActive] = useState(false);
	if (
		revisionId != null &&
		!file.versions.some((version) => version.id === revisionId)
	) {
		return <p role="alert">{t("content_revision_unavailable")}</p>;
	}
	const targetId = file.team_id ?? file.owner_user_id;
	if (targetId == null) return <p>{t("content_access_failed")}</p>;
	if (!active)
		return (
			<Button variant="outline" onClick={() => setActive(true)}>
				{t("content_access_download")}
			</Button>
		);
	return (
		<AdminContentAccessGate
			key={`${file.id}:${targetId}`}
			scopeKind={file.team_id == null ? "personal" : "team"}
			targetId={targetId}
		>
			{(accessId) => (
				<Downloads accessId={accessId} file={file} revisionId={revisionId} />
			)}
		</AdminContentAccessGate>
	);
}

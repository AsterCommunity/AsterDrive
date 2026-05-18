import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { handleApiError } from "@/hooks/useApiError";
import { logger } from "@/lib/logger";
import {
	getPublicSiteUrls,
	normalizePublicSiteUrl,
	setPublicSiteUrls,
} from "@/lib/publicSiteUrl";
import { adminConfigService } from "@/services/adminService";
import { useBrandingStore } from "@/stores/brandingStore";

const PUBLIC_SITE_URL_KEY = "public_site_url";
const ADMIN_SITE_SETTINGS_PATH = "/admin/settings/general";

function syncPublicSiteUrlRuntime(value: string[] | null | undefined) {
	const siteUrl = setPublicSiteUrls(value);
	useBrandingStore.setState({ siteUrl });
	return getPublicSiteUrls();
}

function normalizeConfigValue(value: unknown) {
	return Array.isArray(value) && value.every((item) => typeof item === "string")
		? value
		: [];
}

export function AdminSiteUrlMismatchPrompt() {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	const isBrandingLoaded = useBrandingStore((state) => state.isLoaded);
	const configuredSiteUrl = useBrandingStore((state) => state.siteUrl);
	const siteUrlPromptCheckedRef = useRef(false);
	const [siteUrlMismatchDialogOpen, setSiteUrlMismatchDialogOpen] =
		useState(false);
	const [siteUrlMismatchCurrentOrigin, setSiteUrlMismatchCurrentOrigin] =
		useState<string | null>(null);
	const [
		siteUrlMismatchConfiguredOrigins,
		setSiteUrlMismatchConfiguredOrigins,
	] = useState<string[] | null>(null);
	const configuredSiteUrlDescription = siteUrlMismatchConfiguredOrigins
		? siteUrlMismatchConfiguredOrigins.length > 0
			? siteUrlMismatchConfiguredOrigins.join(", ")
			: t("site_url_mismatch_not_set")
		: configuredSiteUrl;

	useEffect(() => {
		if (
			siteUrlPromptCheckedRef.current ||
			!isBrandingLoaded ||
			typeof window === "undefined"
		) {
			return;
		}

		let cancelled = false;
		const currentOrigin = normalizePublicSiteUrl(window.location.origin);
		if (!currentOrigin) {
			siteUrlPromptCheckedRef.current = true;
			return;
		}

		void (async () => {
			try {
				const config = await adminConfigService.get(PUBLIC_SITE_URL_KEY);
				if (cancelled) return;

				siteUrlPromptCheckedRef.current = true;
				const configuredOrigins = syncPublicSiteUrlRuntime(
					normalizeConfigValue(config.value),
				);
				if (configuredOrigins.includes(currentOrigin)) {
					return;
				}

				if (configuredOrigins.length > 1) {
					navigate(ADMIN_SITE_SETTINGS_PATH, { replace: true });
					return;
				}

				setSiteUrlMismatchConfiguredOrigins(configuredOrigins);
				setSiteUrlMismatchCurrentOrigin(currentOrigin);
				setSiteUrlMismatchDialogOpen(true);
			} catch (error) {
				if (cancelled) return;
				siteUrlPromptCheckedRef.current = true;
				logger.warn(
					"failed to check public_site_url before admin prompt",
					error,
				);
			}
		})();

		return () => {
			cancelled = true;
		};
	}, [isBrandingLoaded, navigate]);

	const handleUpdatePublicSiteUrl = useCallback(async () => {
		if (!siteUrlMismatchCurrentOrigin) {
			return;
		}

		try {
			const nextValue = [
				...getPublicSiteUrls().filter(
					(origin) => origin !== siteUrlMismatchCurrentOrigin,
				),
				siteUrlMismatchCurrentOrigin,
			];
			const savedConfig = await adminConfigService.set(
				PUBLIC_SITE_URL_KEY,
				nextValue,
			);
			syncPublicSiteUrlRuntime(
				Array.isArray(savedConfig.value) ? savedConfig.value : [],
			);
			toast.success(t("settings_saved"));
		} catch (error) {
			handleApiError(error);
		}
	}, [siteUrlMismatchCurrentOrigin, t]);

	return (
		<ConfirmDialog
			open={siteUrlMismatchDialogOpen}
			onOpenChange={setSiteUrlMismatchDialogOpen}
			title={t("site_url_mismatch_title")}
			description={
				siteUrlMismatchCurrentOrigin
					? t("site_url_mismatch_description", {
							configured:
								configuredSiteUrlDescription ?? t("site_url_mismatch_not_set"),
							current: siteUrlMismatchCurrentOrigin,
						})
					: undefined
			}
			confirmLabel={t("site_url_mismatch_confirm")}
			onConfirm={() => {
				void handleUpdatePublicSiteUrl();
			}}
		/>
	);
}

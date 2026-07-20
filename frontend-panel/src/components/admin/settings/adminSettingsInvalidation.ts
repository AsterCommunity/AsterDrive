import { MEDIA_PROCESSING_CONFIG_KEY } from "@/components/admin/mediaProcessingConfigEditorShared";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { useMediaDataSupportStore } from "@/stores/mediaDataSupportStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import type {
	ConfigInvalidationTarget,
	ConfigSchemaItem,
	SystemConfig,
} from "@/types/api";

const PUBLIC_BRANDING_CONFIG_KEYS = new Set([
	"public_site_url",
	"auth_allow_user_registration",
	"branding_title",
	"branding_description",
	"branding_favicon_url",
	"branding_wordmark_dark_url",
	"branding_wordmark_light_url",
]);

const FRONTEND_CONFIG_KEYS = new Set([
	...PUBLIC_BRANDING_CONFIG_KEYS,
	"archive_download_user_enabled",
	"archive_download_share_enabled",
	"frontend_image_preview_preference",
]);

const MEDIA_DATA_SUPPORT_CONFIG_KEYS = new Set([
	MEDIA_PROCESSING_CONFIG_KEY,
	"media_metadata_enabled",
	"media_metadata_max_source_bytes",
]);

export type AdminSettingsInvalidationTarget = ConfigInvalidationTarget;

export function collectAdminSettingsInvalidationTargets(
	changedConfigs: Pick<SystemConfig, "key">[],
	schemas?: Pick<ConfigSchemaItem, "key" | "invalidates">[],
) {
	const targets = new Set<AdminSettingsInvalidationTarget>();
	const schemaInvalidatesByKey = new Map(
		schemas
			?.filter((schema) => Object.hasOwn(schema, "invalidates"))
			.map((schema) => [schema.key, schema.invalidates ?? []] as const),
	);

	for (const config of changedConfigs) {
		const schemaInvalidates = schemaInvalidatesByKey.get(config.key);
		if (schemaInvalidates !== undefined) {
			for (const target of schemaInvalidates) {
				targets.add(target);
			}
			continue;
		}

		if (FRONTEND_CONFIG_KEYS.has(config.key)) {
			targets.add("frontend_config");
		}
		if (config.key === PREVIEW_APPS_CONFIG_KEY) {
			targets.add("preview_apps");
		}
		if (config.key === MEDIA_PROCESSING_CONFIG_KEY) {
			targets.add("thumbnail_support");
		}
		if (MEDIA_DATA_SUPPORT_CONFIG_KEYS.has(config.key)) {
			targets.add("media_data_support");
		}
	}

	return targets;
}

export function applyAdminSettingsInvalidations(
	targets: ReadonlySet<AdminSettingsInvalidationTarget>,
) {
	if (targets.has("frontend_config")) {
		useFrontendConfigStore.getState().invalidate();
		void useFrontendConfigStore.getState().load({ force: true });
	}
	if (targets.has("preview_apps")) {
		usePreviewAppStore.getState().invalidate();
		void usePreviewAppStore.getState().load({ force: true });
	}
	if (targets.has("thumbnail_support")) {
		useThumbnailSupportStore.getState().invalidate();
		void useThumbnailSupportStore.getState().load({ force: true });
	}
	if (targets.has("media_data_support")) {
		useMediaDataSupportStore.getState().invalidate();
		void useMediaDataSupportStore.getState().load({ force: true });
	}
}

import { MediaProcessingConfigEditor } from "@/components/admin/MediaProcessingConfigEditor";
import { MEDIA_PROCESSING_CONFIG_KEY } from "@/components/admin/mediaProcessingConfigEditorShared";
import { OfflineDownloadEngineRegistryEditor } from "@/components/admin/OfflineDownloadEngineRegistryEditor";
import { OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY } from "@/components/admin/offlineDownloadEngineRegistryShared";
import { PreviewAppsConfigEditor } from "@/components/admin/PreviewAppsConfigEditor";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";

export interface AdminSettingsEditorActions {
	buildWopiDiscoveryPreviewConfig: (options: {
		discoveryUrl: string;
		value: string;
	}) => Promise<string>;
	testAria2Rpc: (value: string) => Promise<void>;
	testFfmpegCliCommand: (value: string) => Promise<void>;
	testFfprobeCliCommand: (value: string) => Promise<void>;
	testVipsCliCommand: (value: string) => Promise<void>;
}

export interface AdminSettingsEditorRenderContext {
	actions: AdminSettingsEditorActions;
	onChange: (value: string) => void;
	value: string;
}

interface AdminSettingsEditorDescriptor {
	key: string;
	render: (context: AdminSettingsEditorRenderContext) => React.ReactNode;
}

const ADMIN_SETTINGS_EDITOR_REGISTRY: AdminSettingsEditorDescriptor[] = [
	{
		key: PREVIEW_APPS_CONFIG_KEY,
		render: ({ actions, onChange, value }) => (
			<PreviewAppsConfigEditor
				onBuildWopiDiscoveryPreviewConfig={
					actions.buildWopiDiscoveryPreviewConfig
				}
				value={value}
				onChange={onChange}
			/>
		),
	},
	{
		key: MEDIA_PROCESSING_CONFIG_KEY,
		render: ({ actions, onChange, value }) => (
			<MediaProcessingConfigEditor
				onTestFfmpegCliCommand={actions.testFfmpegCliCommand}
				onTestFfprobeCliCommand={actions.testFfprobeCliCommand}
				onTestVipsCliCommand={actions.testVipsCliCommand}
				value={value}
				onChange={onChange}
			/>
		),
	},
	{
		key: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
		render: ({ actions, onChange, value }) => (
			<OfflineDownloadEngineRegistryEditor
				onTestAria2Rpc={actions.testAria2Rpc}
				value={value}
				onChange={onChange}
			/>
		),
	},
];

export function getAdminSettingsEditorDescriptor(key: string) {
	return (
		ADMIN_SETTINGS_EDITOR_REGISTRY.find(
			(descriptor) => descriptor.key === key,
		) ?? null
	);
}

export function hasAdminSettingsEditor(key: string) {
	return getAdminSettingsEditorDescriptor(key) !== null;
}

export function renderAdminSettingsEditor(
	key: string,
	context: AdminSettingsEditorRenderContext,
) {
	return getAdminSettingsEditorDescriptor(key)?.render(context) ?? null;
}

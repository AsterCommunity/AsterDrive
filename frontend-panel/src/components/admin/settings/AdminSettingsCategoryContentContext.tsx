import { createContext, type Dispatch, type SetStateAction, use } from "react";
import type {
	ConfigDraftValue,
	NewCustomDraft,
	SizeDisplayUnitValue,
	SystemSubcategoryGroup,
	TimeDisplayUnitValue,
} from "@/components/admin/settings/adminSettingsContentShared";
import type {
	ConfigSchemaItem,
	ConfigVisibility,
	SystemConfig,
} from "@/types/api";

type TranslationFn = (key: string, options?: Record<string, unknown>) => string;

export interface AdminSettingsCategoryContentProps {
	activeTab: string;
	addCustomDraftRow: () => void;
	category: string;
	configValidationErrors: Map<string, string>;
	deletedCustomConfigs: SystemConfig[];
	displayUnits: Partial<
		Record<string, TimeDisplayUnitValue | SizeDisplayUnitValue>
	>;
	editorTheme: "vs" | "vs-dark";
	expandedSubcategoryGroups: Record<string, boolean>;
	expandedTemplateGroups: Record<string, boolean>;
	getCategoryDescription: (category: string) => string | undefined;
	getCategoryLabel: (category: string) => string;
	getDraftValue: (config: SystemConfig) => ConfigDraftValue;
	getDraftValueByKey: (key: string) => ConfigDraftValue | undefined;
	getMailTemplateGroupLabel: (groupId: string) => string;
	getSubcategoryDescription: (
		category: string,
		subcategory?: string,
	) => string | undefined;
	getSubcategoryLabel: (category: string, subcategory?: string) => string;
	getSystemConfigDescription: (config: SystemConfig) => string | undefined;
	getSystemConfigLabel: (config: SystemConfig) => string;
	getSystemConfigSchema: (config: SystemConfig) => ConfigSchemaItem | undefined;
	getSystemConfigSchemas: () => ConfigSchemaItem[];
	getCustomVisibilityDraft: (config: SystemConfig) => ConfigVisibility;
	handleBuildWopiDiscoveryPreviewConfig: (options: {
		discoveryUrl: string;
		value: string;
	}) => Promise<string>;
	handleTestFfmpegCliCommand: (value: string) => Promise<void>;
	handleTestFfprobeCliCommand: (value: string) => Promise<void>;
	handleTestAria2Rpc: (value: string) => Promise<void>;
	handleTestVipsCliCommand: (value: string) => Promise<void>;
	isMobileNavigation: boolean;
	markCustomDeleted: (key: string) => void;
	newCustomRowErrors: Map<string, string>;
	newCustomRows: NewCustomDraft[];
	openTemplateVariablesDialog: (config: SystemConfig) => void;
	openTestEmailDialog: () => void;
	removeNewCustomRow: (id: string) => void;
	restoreDeletedCustom: (key: string) => void;
	setDisplayUnits: Dispatch<
		SetStateAction<
			Partial<Record<string, TimeDisplayUnitValue | SizeDisplayUnitValue>>
		>
	>;
	systemGroups: Record<string, SystemConfig[]>;
	systemSubcategoryGroups: Record<string, SystemSubcategoryGroup[]>;
	t: TranslationFn;
	tabDirection: "forward" | "backward";
	toggleSubcategoryGroup: (groupKey: string, nextExpanded: boolean) => void;
	toggleTemplateGroup: (groupKey: string, nextExpanded: boolean) => void;
	updateDraftValue: (key: string, value: ConfigDraftValue) => void;
	updateCustomVisibilityDraft: (
		key: string,
		visibility: ConfigVisibility,
	) => void;
	navigateToMailSettings: () => void;
	updateNewCustomRow: (
		id: string,
		field: keyof Omit<NewCustomDraft, "id">,
		value: string,
	) => void;
	visibleCustomConfigs: SystemConfig[];
}

const AdminSettingsCategoryContentContext =
	createContext<AdminSettingsCategoryContentProps | null>(null);

export function AdminSettingsCategoryContentProvider({
	children,
	value,
}: {
	children: React.ReactNode;
	value: AdminSettingsCategoryContentProps;
}) {
	return (
		<AdminSettingsCategoryContentContext.Provider value={value}>
			{children}
		</AdminSettingsCategoryContentContext.Provider>
	);
}

export function useAdminSettingsCategoryContent() {
	const context = use(AdminSettingsCategoryContentContext);
	if (!context) {
		throw new Error("AdminSettingsCategoryContent context is missing");
	}
	return context;
}

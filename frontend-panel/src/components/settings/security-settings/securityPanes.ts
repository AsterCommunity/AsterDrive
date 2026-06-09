import type { IconName } from "@/components/ui/icon";

export type SecurityPane =
	| "account"
	| "mfa"
	| "passkeys"
	| "external"
	| "sessions";

export const SECURITY_PANES: Array<{
	descriptionKey: string;
	icon: IconName;
	labelKey: string;
	value: SecurityPane;
}> = [
	{
		descriptionKey: "settings:settings_security_tab_account_desc",
		icon: "Lock",
		labelKey: "settings:settings_security_tab_account",
		value: "account",
	},
	{
		descriptionKey: "settings:settings_security_tab_passkeys_desc",
		icon: "Shield",
		labelKey: "settings:settings_security_tab_passkeys",
		value: "passkeys",
	},
	{
		descriptionKey: "settings:settings_security_tab_mfa_desc",
		icon: "Key",
		labelKey: "settings:settings_security_tab_mfa",
		value: "mfa",
	},
	{
		descriptionKey: "settings:settings_security_tab_external_desc",
		icon: "Globe",
		labelKey: "settings:settings_security_tab_external",
		value: "external",
	},
	{
		descriptionKey: "settings:settings_security_tab_sessions_desc",
		icon: "Monitor",
		labelKey: "settings:settings_security_tab_sessions",
		value: "sessions",
	},
];

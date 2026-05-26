import {
	type ConfigDraftValue,
	configValueToString,
} from "@/components/admin/settings/adminSettingsContentShared";

export const MAIL_SMTP_HOST_KEY = "mail_smtp_host";
export const MAIL_FROM_ADDRESS_KEY = "mail_from_address";
export const MAIL_SMTP_USERNAME_KEY = "mail_smtp_username";
export const MAIL_SMTP_PASSWORD_KEY = "mail_smtp_password";

export const MAIL_DELIVERY_CONFIG_KEYS = new Set([
	MAIL_SMTP_HOST_KEY,
	MAIL_FROM_ADDRESS_KEY,
	MAIL_SMTP_USERNAME_KEY,
	MAIL_SMTP_PASSWORD_KEY,
]);

export function isMailDeliveryConfigReady(
	readValue: (key: string) => ConfigDraftValue | undefined,
) {
	const smtpHost = configValueToString(readValue(MAIL_SMTP_HOST_KEY)).trim();
	const fromAddress = configValueToString(
		readValue(MAIL_FROM_ADDRESS_KEY),
	).trim();
	const smtpUsername = configValueToString(
		readValue(MAIL_SMTP_USERNAME_KEY),
	).trim();
	const smtpPassword = configValueToString(
		readValue(MAIL_SMTP_PASSWORD_KEY),
	).trim();

	return (
		Boolean(smtpHost) &&
		Boolean(fromAddress) &&
		Boolean(smtpUsername) === Boolean(smtpPassword)
	);
}

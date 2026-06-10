import i18next from "i18next";
import { z } from "zod/v4";

function translateValidation(key: string): string {
	return i18next.t(key, { ns: "validation" });
}

export const usernameSchema = z
	.string()
	.min(4, translateValidation("username_length"))
	.max(16, translateValidation("username_length"))
	.regex(/^[a-zA-Z0-9_-]+$/, translateValidation("username_chars"));

export const emailSchema = z
	.string()
	.max(254, translateValidation("email_too_long"))
	.regex(/^[^@]+@[^@]+\.[^@]+$/, translateValidation("email_format"));

export const existingPasswordSchema = z
	.string()
	.min(1, translateValidation("password_required"));

export const passwordSchema = z
	.string()
	.min(8, translateValidation("password_min"))
	.max(128, translateValidation("password_max"));

function refinePasswordChangeMatch(
	{
		confirmPassword,
		currentPassword,
		newPassword,
	}: {
		confirmPassword: string;
		currentPassword: string;
		newPassword: string;
	},
	ctx: z.RefinementCtx,
) {
	if (
		currentPassword.length > 0 &&
		newPassword.length > 0 &&
		newPassword === currentPassword
	) {
		ctx.addIssue({
			code: "custom",
			message: translateValidation("password_same_as_current"),
			path: ["newPassword"],
		});
	}
	if (newPassword.length > 0 && confirmPassword !== newPassword) {
		ctx.addIssue({
			code: "custom",
			message: translateValidation("password_confirm_mismatch"),
			path: ["confirmPassword"],
		});
	}
}

export const passwordChangeMatchSchema = z
	.object({
		confirmPassword: z.string(),
		currentPassword: z.string(),
		newPassword: z.string(),
	})
	.superRefine(refinePasswordChangeMatch);

export const passwordChangeSchema = z
	.object({
		confirmPassword: z.string(),
		currentPassword: existingPasswordSchema,
		newPassword: passwordSchema,
	})
	.superRefine(refinePasswordChangeMatch);

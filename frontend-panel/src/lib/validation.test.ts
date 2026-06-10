import { describe, expect, it, vi } from "vitest";

vi.mock("i18next", () => ({
	default: {
		t: (key: string) => `validation:${key}`,
	},
}));

describe("validation schemas", () => {
	it("accepts valid values", async () => {
		const {
			emailSchema,
			existingPasswordSchema,
			passwordSchema,
			usernameSchema,
		} = await import("@/lib/validation");

		expect(usernameSchema.safeParse("user_1").success).toBe(true);
		expect(emailSchema.safeParse("user@example.com").success).toBe(true);
		expect(existingPasswordSchema.safeParse("pass123").success).toBe(true);
		expect(passwordSchema.safeParse("secret12").success).toBe(true);
	});

	it("returns translated username and email validation messages", async () => {
		const { emailSchema, usernameSchema } = await import("@/lib/validation");

		const username = usernameSchema.safeParse("a!");
		const email = emailSchema.safeParse("bad-email");

		expect(username.success).toBe(false);
		expect(username.error.issues[0]?.message).toBe(
			"validation:username_length",
		);
		expect(email.success).toBe(false);
		expect(email.error.issues[0]?.message).toBe("validation:email_format");
	});

	it("enforces password length boundaries", async () => {
		const { passwordSchema } = await import("@/lib/validation");

		expect(passwordSchema.safeParse("1234567").success).toBe(false);
		expect(passwordSchema.safeParse("x".repeat(129)).success).toBe(false);
	});

	it("returns translated password validation messages", async () => {
		const { existingPasswordSchema, passwordSchema } = await import(
			"@/lib/validation"
		);

		const missingExistingPassword = existingPasswordSchema.safeParse("");
		const shortPassword = passwordSchema.safeParse("1234567");
		const longPassword = passwordSchema.safeParse("x".repeat(129));

		expect(missingExistingPassword.success).toBe(false);
		expect(missingExistingPassword.error.issues[0]?.message).toBe(
			"validation:password_required",
		);
		expect(shortPassword.success).toBe(false);
		expect(shortPassword.error.issues[0]?.message).toBe(
			"validation:password_min",
		);
		expect(longPassword.success).toBe(false);
		expect(longPassword.error.issues[0]?.message).toBe(
			"validation:password_max",
		);
	});

	it("validates password change match rules independently from required fields", async () => {
		const { passwordChangeMatchSchema, passwordChangeSchema } = await import(
			"@/lib/validation"
		);

		const mismatch = passwordChangeMatchSchema.safeParse({
			confirmPassword: "different456",
			currentPassword: "",
			newPassword: "newsecret456",
		});
		const reused = passwordChangeSchema.safeParse({
			confirmPassword: "temporary123",
			currentPassword: "temporary123",
			newPassword: "temporary123",
		});

		expect(mismatch.success).toBe(false);
		expect(mismatch.error.issues[0]?.message).toBe(
			"validation:password_confirm_mismatch",
		);
		expect(reused.success).toBe(false);
		expect(
			reused.error.issues.some(
				(issue) =>
					issue.path.join(".") === "newPassword" &&
					issue.message === "validation:password_same_as_current",
			),
		).toBe(true);
	});
});

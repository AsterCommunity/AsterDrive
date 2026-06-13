import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ActivationResendRequestPanel } from "@/pages/login/ActivationResendRequestPanel";

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button type={type ?? "button"} disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({ ...props }: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
	}: {
		children: React.ReactNode;
		htmlFor?: string;
	}) => <label htmlFor={htmlFor}>{children}</label>,
}));

const emailSchema = {
	safeParse: (value: string) =>
		value.includes("@")
			? { success: true }
			: {
					error: { issues: [{ message: "invalid-email" }] },
					success: false,
				},
};

describe("ActivationResendRequestPanel", () => {
	it("validates email edits and submits/back-navigates", async () => {
		const onBack = vi.fn();
		const onEmailChange = vi.fn();
		const onSubmit = vi.fn();
		const user = userEvent.setup();

		function TestPanel() {
			const [email, setEmail] = useState("old@example.com");
			const [emailError, setEmailError] = useState("");

			return (
				<ActivationResendRequestPanel
					email={email}
					emailError={emailError}
					emailSchema={emailSchema}
					requesting={false}
					t={(key) => key}
					onBack={onBack}
					onEmailChange={(value, error) => {
						onEmailChange(value, error);
						setEmail(value);
						setEmailError(error);
					}}
					onSubmit={onSubmit}
				/>
			);
		}

		render(<TestPanel />);

		const emailInput = screen.getByLabelText("core:email");
		await user.clear(emailInput);
		await user.type(emailInput, "invalid");
		expect(onEmailChange).toHaveBeenLastCalledWith("invalid", "invalid-email");

		await user.clear(emailInput);
		await user.type(emailInput, "user@example.com");
		await user.click(screen.getByRole("button", { name: /resend_activation/ }));
		await user.click(screen.getByRole("button", { name: /back_to_sign_in/ }));

		expect(onEmailChange).toHaveBeenLastCalledWith("user@example.com", "");
		expect(onSubmit).toHaveBeenCalledTimes(1);
		expect(onBack).toHaveBeenCalledTimes(1);
	});

	it("shows errors and disables submit while empty or requesting", () => {
		const { rerender } = render(
			<ActivationResendRequestPanel
				email="invalid"
				emailError="invalid-email"
				emailSchema={emailSchema}
				requesting={false}
				t={(key) => key}
				onBack={vi.fn()}
				onEmailChange={vi.fn()}
				onSubmit={vi.fn()}
			/>,
		);

		const error = screen.getByText("invalid-email");
		expect(error).toHaveAttribute("id", "activation-resend-email-error");
		expect(error).toHaveAttribute("role", "alert");
		expect(screen.getByLabelText("core:email")).toHaveAttribute(
			"aria-describedby",
			"activation-resend-email-error",
		);
		expect(
			screen.getByRole("button", { name: /resend_activation/ }),
		).toBeDisabled();

		rerender(
			<ActivationResendRequestPanel
				email="user@example.com"
				emailError=""
				emailSchema={emailSchema}
				requesting
				t={(key) => key}
				onBack={vi.fn()}
				onEmailChange={vi.fn()}
				onSubmit={vi.fn()}
			/>,
		);

		expect(
			screen.getByRole("button", { name: /resending_activation/ }),
		).toBeDisabled();
		expect(screen.getByText("Spinner")).toBeInTheDocument();
	});
});

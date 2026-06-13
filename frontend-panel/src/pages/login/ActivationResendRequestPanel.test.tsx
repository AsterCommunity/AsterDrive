import { fireEvent, render, screen } from "@testing-library/react";
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
	it("validates email edits and submits/back-navigates", () => {
		const onBack = vi.fn();
		const onEmailChange = vi.fn();
		const onSubmit = vi.fn();

		render(
			<ActivationResendRequestPanel
				email="old@example.com"
				emailError=""
				emailSchema={emailSchema}
				requesting={false}
				t={(key) => key}
				onBack={onBack}
				onEmailChange={onEmailChange}
				onSubmit={onSubmit}
			/>,
		);

		const emailInput = screen.getByLabelText("core:email");
		fireEvent.change(emailInput, { target: { value: "invalid" } });
		fireEvent.change(emailInput, { target: { value: "user@example.com" } });
		fireEvent.click(screen.getByRole("button", { name: /resend_activation/ }));
		fireEvent.click(screen.getByRole("button", { name: /back_to_sign_in/ }));

		expect(onEmailChange).toHaveBeenNthCalledWith(
			1,
			"invalid",
			"invalid-email",
		);
		expect(onEmailChange).toHaveBeenNthCalledWith(2, "user@example.com", "");
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

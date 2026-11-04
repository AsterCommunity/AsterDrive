import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { AdminDebouncedFilterInput } from "@/components/admin/AdminDebouncedFilterInput";

afterEach(() => vi.useRealTimers());

it("keeps newer typing when its previous emission returns as the value", () => {
	vi.useFakeTimers();
	const onChange = vi.fn();
	const { rerender } = render(
		<AdminDebouncedFilterInput
			value=""
			onChange={onChange}
			aria-label="name"
		/>,
	);
	const input = screen.getByRole("textbox", { name: "name" });

	fireEvent.change(input, { target: { value: "ab" } });
	act(() => vi.advanceTimersByTime(300));
	expect(onChange).toHaveBeenLastCalledWith("ab");
	fireEvent.change(input, { target: { value: "abc" } });
	rerender(
		<AdminDebouncedFilterInput
			value="ab"
			onChange={onChange}
			aria-label="name"
		/>,
	);
	expect(input).toHaveValue("abc");
	act(() => vi.advanceTimersByTime(300));
	expect(onChange).toHaveBeenLastCalledWith("abc");

	rerender(
		<AdminDebouncedFilterInput
			value="reset"
			onChange={onChange}
			aria-label="name"
		/>,
	);
	expect(input).toHaveValue("reset");
});

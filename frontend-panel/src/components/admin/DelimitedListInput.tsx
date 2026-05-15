import { type ComponentProps, useEffect, useState } from "react";
import { Input } from "@/components/ui/input";

interface DelimitedListInputProps
	extends Omit<ComponentProps<"input">, "onChange" | "value"> {
	formatValue: (values: string[]) => string;
	onValueChange: (values: string[]) => void;
	parseValue: (value: string) => string[];
	values: string[];
}

export function DelimitedListInput({
	formatValue,
	onBlur,
	onFocus,
	onValueChange,
	parseValue,
	values,
	...props
}: DelimitedListInputProps) {
	const formattedValue = formatValue(values);
	const [draftValue, setDraftValue] = useState(formattedValue);
	const [focused, setFocused] = useState(false);

	useEffect(() => {
		if (!focused) {
			setDraftValue(formattedValue);
		}
	}, [focused, formattedValue]);

	return (
		<Input
			{...props}
			value={focused ? draftValue : formattedValue}
			onChange={(event) => {
				const nextValue = event.target.value;
				setDraftValue(nextValue);
				onValueChange(parseValue(nextValue));
			}}
			onFocus={(event) => {
				setFocused(true);
				setDraftValue(event.currentTarget.value);
				onFocus?.(event);
			}}
			onBlur={(event) => {
				const normalizedValue = formatValue(
					parseValue(event.currentTarget.value),
				);
				setFocused(false);
				setDraftValue(normalizedValue);
				onBlur?.(event);
			}}
		/>
	);
}

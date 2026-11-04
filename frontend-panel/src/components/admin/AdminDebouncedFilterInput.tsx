import { useEffect, useRef, useState } from "react";
import { Input } from "@/components/ui/input";

export function AdminDebouncedFilterInput({
	value,
	onChange,
	...props
}: Omit<React.ComponentProps<typeof Input>, "onChange" | "value"> & {
	value: string;
	onChange: (value: string) => void;
}) {
	const [draft, setDraft] = useState(value);
	const onChangeRef = useRef(onChange);
	onChangeRef.current = onChange;
	useEffect(() => setDraft(value), [value]);
	useEffect(() => {
		if (draft === value) return;
		const timer = window.setTimeout(() => onChangeRef.current(draft), 300);
		return () => window.clearTimeout(timer);
	}, [draft, value]);
	return (
		<Input
			{...props}
			value={draft}
			onChange={(event) => setDraft(event.target.value)}
		/>
	);
}

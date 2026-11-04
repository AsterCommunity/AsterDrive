import { useEffect, useState } from "react";
import { createDebouncedTask } from "@/lib/debounce";

export function useDebouncedValue<T>(value: T, delay = 350): T {
	const [debounced, setDebounced] = useState(value);
	useEffect(() => {
		const task = createDebouncedTask(delay);
		task.schedule(() => setDebounced(value));
		return task.cancel;
	}, [delay, value]);
	return debounced;
}

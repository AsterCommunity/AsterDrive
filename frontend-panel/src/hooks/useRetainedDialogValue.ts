import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";

interface UseRetainedDialogValueResult<T> {
	handleOpenChangeComplete: (open: boolean) => void;
	retainedValue: T | null;
}

export function useRetainedDialogValue<T>(
	value: T | null,
	open: boolean,
): UseRetainedDialogValueResult<T> {
	const openRef = useRef(open);
	useLayoutEffect(() => {
		openRef.current = open;
	}, [open]);
	const [retainedValue, setRetainedValue] = useState<T | null>(value);

	useEffect(() => {
		setRetainedValue((current) => {
			if (value !== null && current !== value) {
				return value;
			}
			if (value === null && open && current !== null) {
				return null;
			}
			return current;
		});
	}, [value, open]);

	const visibleValue = value ?? (open ? null : retainedValue);

	const handleOpenChangeComplete = useCallback((nextOpen: boolean) => {
		if (!nextOpen && !openRef.current) {
			setRetainedValue(null);
		}
	}, []);

	return { retainedValue: visibleValue, handleOpenChangeComplete };
}

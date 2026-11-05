import { useCallback, useEffect, useRef, useState } from "react";
import { handleApiError } from "@/hooks/useApiError";

export function useApiList<T>(
	fetcher: () => Promise<{ items: T[]; total?: number }>,
	deps: unknown[] = [],
) {
	const [items, setItems] = useState<T[]>([]);
	const [total, setTotal] = useState(0);
	const [loading, setLoading] = useState(true);
	const requestIdRef = useRef(0);

	const load = useCallback(async () => {
		const requestId = ++requestIdRef.current;
		try {
			setLoading(true);
			const data = await fetcher();
			if (requestId !== requestIdRef.current) {
				return;
			}
			setItems(data.items);
			if (data.total !== undefined) setTotal(data.total);
		} catch (e) {
			if (requestId === requestIdRef.current) {
				handleApiError(e);
			}
		} finally {
			if (requestId === requestIdRef.current) {
				setLoading(false);
			}
		}
		// biome-ignore lint/correctness/useExhaustiveDependencies: deps is a dynamic parameter by design
	}, deps);

	useEffect(() => {
		void load().catch(() => {});
		return () => {
			requestIdRef.current += 1;
		};
	}, [load]);

	return { items, setItems, total, setTotal, loading, reload: load };
}

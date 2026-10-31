import { useCallback, useRef, useState } from "react";
import { useDialogActivity } from "@/components/ui/dialog-lifecycle";

export function usePendingAction() {
	const isCurrent = useDialogActivity();
	const [pending, setPending] = useState(false);
	const pendingRef = useRef(false);

	const runWithPending = useCallback(
		async <T>(action: () => Promise<T>) => {
			if (pendingRef.current) {
				return {
					entered: false as const,
					value: undefined,
				};
			}

			pendingRef.current = true;
			setPending(true);
			try {
				const value = await action();
				return {
					entered: true as const,
					value,
				};
			} finally {
				pendingRef.current = false;
				if (isCurrent()) setPending(false);
			}
		},
		[isCurrent],
	);

	const clearPending = useCallback(() => {
		pendingRef.current = false;
		setPending(false);
	}, []);

	return {
		clearPending,
		pending,
		runWithPending,
	};
}

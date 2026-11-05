import { useEffect, useRef, useState } from "react";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";

// The URL remains the committed query. Draft strings retain intermediate input,
// including whitespace and decimals, until a valid complete patch can be sent.
export function useDebouncedQueryDraft<Draft>({
	initial,
	sourceKey,
	preserveUnchanged = false,
	normalize,
	onCommit,
}: {
	initial: Draft;
	sourceKey: string;
	preserveUnchanged?: boolean;
	normalize: (draft: Draft) => Draft | null;
	onCommit: (draft: Draft) => void;
}) {
	const initialKey = JSON.stringify(initial);
	const [state, setState] = useState({ draft: initial, sourceKey, initialKey });
	const expected = useRef<string | null>(null);
	let current = state;
	if (state.sourceKey !== sourceKey) {
		current = {
			draft:
				expected.current === initialKey ||
				(preserveUnchanged && state.initialKey === initialKey)
					? state.draft
					: initial,
			sourceKey,
			initialKey,
		};
		expected.current = null;
		setState(current);
	}
	const debounced = useDebouncedValue(current);
	useEffect(() => {
		if (debounced.sourceKey !== sourceKey) return;
		const normalized = normalize(debounced.draft);
		if (normalized === null) return;
		const key = JSON.stringify(normalized);
		if (key === initialKey) return;
		expected.current = key;
		onCommit(normalized);
	}, [debounced, initialKey, normalize, onCommit, sourceKey]);
	return {
		draft: current.draft,
		pending: JSON.stringify(normalize(current.draft)) !== initialKey,
		setDraft: (draft: Draft) => setState({ draft, sourceKey, initialKey }),
		resetDraft: () => {
			expected.current = null;
			setState({ draft: initial, sourceKey, initialKey });
		},
	};
}

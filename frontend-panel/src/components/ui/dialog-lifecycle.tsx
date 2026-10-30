import {
	createContext,
	Fragment,
	type ReactNode,
	useCallback,
	useContext,
	useLayoutEffect,
	useMemo,
	useReducer,
	useRef,
	useState,
} from "react";

export type DialogPhase =
	| "closed"
	| "entering"
	| "open"
	| "exiting"
	| "disposed";

interface DialogSession {
	active: boolean;
	complete: (open: boolean) => void;
	isCurrent: () => boolean;
	phase: DialogPhase;
}

const SessionContext = createContext<DialogSession | null>(null);
const CompletionContext = createContext<DialogSession | null>(null);

/** Owns business content, including its drafts and display resources, through exit. */
export function DialogLifecycle<T>({
	open: requestedOpen,
	target,
	identity,
	onOpenChange,
	children,
}: {
	open: boolean;
	target: T;
	identity?: unknown;
	onOpenChange?: (open: boolean) => void;
	children: (session: {
		target: T;
		active: boolean;
		onOpenChange: (open: boolean) => void;
		complete: (open: boolean) => void;
	}) => ReactNode;
}) {
	const parent = useContext(SessionContext);
	const open = requestedOpen && (parent?.active ?? true);
	const parentCurrent = parent?.isCurrent ?? alwaysActive;
	const [presentation, setPresentation] = useState({
		open,
		retained: open ? { target } : null,
		present: open,
		generation: 0,
		identity,
		instance: 0,
		phase: open ? ("entering" as const) : ("closed" as DialogPhase),
	});
	// Capture only active input. A parent may clear its target as soon as it closes.
	if (
		presentation.open !== open ||
		(open &&
			(presentation.retained?.target !== target ||
				!Object.is(presentation.identity, identity)))
	) {
		setPresentation({
			...presentation,
			open,
			retained: open ? { target } : presentation.retained,
			present: open || presentation.present,
			generation:
				presentation.generation +
				Number(
					presentation.open !== open ||
						(open && !Object.is(presentation.identity, identity)),
				),
			identity: open ? identity : presentation.identity,
			instance: presentation.instance + Number(open && !presentation.open),
			phase:
				presentation.open === open
					? presentation.phase
					: open
						? "entering"
						: "exiting",
		});
	}
	const { generation, present, phase } = presentation;
	const [closeRequest, resyncAfterCloseRequest] = useReducer(
		(value: number) => value + 1,
		0,
	);
	const live = useRef({ active: false, generation, identity });
	useLayoutEffect(() => {
		// A rejected close does not change open; resync its optimistic invalidation too.
		void closeRequest;
		live.current = { active: open, generation, identity };
		return () => {
			live.current.active = false;
		};
	}, [open, generation, identity, closeRequest]);
	const isCurrent = useCallback(
		() =>
			parentCurrent() &&
			live.current.active &&
			live.current.generation === generation &&
			Object.is(live.current.identity, identity),
		[generation, identity, parentCurrent],
	);
	const complete = useCallback(
		(nextOpen: boolean) => {
			if (live.current.generation !== generation) return;
			setPresentation((current) => {
				if (current.generation !== generation || current.open !== nextOpen) {
					return current;
				}
				if (current.phase === (nextOpen ? "open" : "disposed")) return current;
				return {
					...current,
					present: nextOpen,
					retained: nextOpen ? current.retained : null,
					phase: nextOpen ? "open" : "disposed",
				};
			});
		},
		[generation],
	);
	const changeOpen = useCallback(
		(nextOpen: boolean) => {
			if (!isCurrent()) return;
			if (!nextOpen) live.current.active = false;
			onOpenChange?.(nextOpen);
			if (!nextOpen) resyncAfterCloseRequest();
		},
		[isCurrent, onOpenChange],
	);
	const session = useMemo(
		() => ({ active: open, complete, isCurrent, phase }),
		[open, complete, isCurrent, phase],
	);
	if (!present || !presentation.retained) return null;
	return (
		<SessionContext value={session}>
			<CompletionContext value={session}>
				<Fragment key={presentation.instance}>
					{children({
						target: presentation.retained.target,
						active: open,
						onOpenChange: changeOpen,
						complete,
					})}
				</Fragment>
			</CompletionContext>
		</SessionContext>
	);
}

/** Capture before awaiting work; success side effects still run, stale UI work does not. */
export function useDialogActivity(identity?: unknown, active = true) {
	const session = useContext(SessionContext);
	const token = useMemo(() => ({ active, identity }), [active, identity]);
	useLayoutEffect(() => {
		token.active = active;
		return () => {
			token.active = false;
		};
	}, [token, active]);
	const isCurrent = session?.isCurrent ?? alwaysActive;
	return useCallback(() => isCurrent() && token.active, [isCurrent, token]);
}

function alwaysActive() {
	return true;
}

export function useDialogPresentation() {
	return useContext(SessionContext);
}

export function useDialogExitComplete() {
	return useContext(CompletionContext)?.complete;
}

/** For non-Base-UI surfaces: use the browser's actual animations, never a CSS timer. */
export function useDialogMotion(open: boolean, enabled = true) {
	const complete = useDialogExitComplete();
	const ref = useRef<HTMLDivElement>(null);
	const [visible, setVisible] = useState(false);
	useLayoutEffect(() => {
		if (!enabled) return;
		if (open && ref.current) void ref.current.offsetWidth;
		setVisible(open);
	}, [open, enabled]);
	useLayoutEffect(() => {
		if (!enabled || visible !== open || !ref.current) return;
		let current = true;
		const animations = ref.current
			.getAnimations({ subtree: true })
			.filter(
				(animation) =>
					animation.effect?.getComputedTiming().iterations !== Infinity,
			);
		void Promise.allSettled(
			animations.map((animation) => animation.finished),
		).then(() => {
			if (current) complete?.(open);
		});
		return () => {
			current = false;
		};
	}, [open, enabled, visible, complete]);
	return { ref, visible };
}

// A nested primitive must not accidentally dispose its parent's presentation.
export function DialogCompletionBoundary({
	children,
}: {
	children: ReactNode;
}) {
	return <CompletionContext value={null}>{children}</CompletionContext>;
}

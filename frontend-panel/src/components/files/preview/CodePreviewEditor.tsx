import {
	Highlight,
	type Token as HighlightToken,
	type RenderProps,
} from "prism-react-renderer";
import {
	type KeyboardEvent,
	useDeferredValue,
	useEffect,
	useRef,
	useState,
} from "react";
import { isImeComposingKeyEvent } from "@/lib/keyboard";
import {
	createEditorPalette,
	ensurePrismLanguage,
	getPrismLanguageConfig,
	hasPrismGrammar,
	normalizePrismLanguage,
	Prism,
} from "./codePreviewPrism";
import { withScopedPrismClassName } from "./prismClassNames";

const KEY_CODE = {
	KeyS: 49,
} as const;

const KEY_MOD = {
	CtrlCmd: 2048,
} as const;

const MONO_FONT_FAMILY =
	'"SFMono-Regular", "SF Mono", "Cascadia Code", "Fira Code", Consolas, "Liberation Mono", Menlo, monospace';

type EditorCommandHandler = () => void;

type EditorLike = {
	addCommand: (keybinding: number, handler: EditorCommandHandler) => void;
};

type EditorShortcutApi = {
	KeyCode: typeof KEY_CODE;
	KeyMod: typeof KEY_MOD;
};

export type CodePreviewEditorMountHandler = (
	editor: EditorLike,
	shortcutApi: EditorShortcutApi,
) => void;

interface CodePreviewEditorProps {
	language: string;
	theme: string;
	value: string;
	onChange?: (value: string) => void;
	onMount?: CodePreviewEditorMountHandler;
	options?: {
		domReadOnly?: boolean;
		fontSize?: number;
		lineNumbers?: "on" | "off";
		minimap?: {
			enabled?: boolean;
		};
		padding?: {
			top?: number;
		};
		readOnly?: boolean;
		renderLineHighlight?: "line" | "none";
		scrollBeyondLastLine?: boolean;
		wordWrap?: "on" | "off";
	};
}

function getKeybindingFromEvent(event: KeyboardEvent<HTMLTextAreaElement>) {
	let keybinding = 0;

	if (event.metaKey || event.ctrlKey) {
		keybinding |= KEY_MOD.CtrlCmd;
	}

	if (event.key.toLowerCase() === "s") {
		keybinding |= KEY_CODE.KeyS;
	}

	return keybinding;
}

function insertTabAtSelection(textarea: HTMLTextAreaElement) {
	const { selectionEnd, selectionStart } = textarea;

	textarea.setRangeText("\t", selectionStart, selectionEnd, "end");

	return textarea.value;
}

function renderLineNumbers(lineCount: number, lineHeight: number) {
	return Array.from({ length: lineCount }, (_, index) => String(index + 1)).map(
		(lineNumber) => (
			<div
				key={lineNumber}
				style={{
					height: lineHeight,
					lineHeight: `${lineHeight}px`,
				}}
			>
				{lineNumber}
			</div>
		),
	);
}

function getHighlightTokenSignature(token: HighlightToken) {
	return `${token.types.join(".")}::${token.content}`;
}

function getHighlightLineSignature(line: HighlightToken[]) {
	const signature = line.map(getHighlightTokenSignature).join("\u0001");

	return signature || "empty-line";
}

function renderHighlightedLines({
	tokens,
	getLineProps,
	getTokenProps,
}: Pick<RenderProps, "tokens" | "getLineProps" | "getTokenProps">) {
	const lineOccurrences = new Map<string, number>();

	return tokens.map((line) => {
		const lineSignature = getHighlightLineSignature(line);
		const lineOccurrence = lineOccurrences.get(lineSignature) ?? 0;
		const lineKey = `${lineSignature}::${lineOccurrence}`;
		const tokenOccurrences = new Map<string, number>();

		lineOccurrences.set(lineSignature, lineOccurrence + 1);

		return (
			<div
				key={lineKey}
				{...withScopedPrismClassName(getLineProps({ line, key: lineKey }))}
			>
				{line.map((token) => {
					const tokenSignature = getHighlightTokenSignature(token);
					const tokenOccurrence = tokenOccurrences.get(tokenSignature) ?? 0;
					const tokenKey = `${lineKey}::${tokenSignature}::${tokenOccurrence}`;

					tokenOccurrences.set(tokenSignature, tokenOccurrence + 1);

					return (
						<span
							key={tokenKey}
							{...withScopedPrismClassName(
								getTokenProps({ token, key: tokenKey }),
							)}
						/>
					);
				})}
			</div>
		);
	});
}

export function CodePreviewEditor({
	language,
	theme,
	value,
	onChange,
	onMount,
	options,
}: CodePreviewEditorProps) {
	const [, setPrismRevision] = useState(0);
	const commandsRef = useRef(new Map<number, EditorCommandHandler>());
	const gutterContentRef = useRef<HTMLDivElement | null>(null);
	const onMountRef = useRef(onMount);
	const overlayContentRef = useRef<HTMLDivElement | null>(null);
	const textareaComposingRef = useRef(false);
	const textareaCompositionEndAtRef = useRef(0);
	const textareaRef = useRef<HTMLTextAreaElement | null>(null);

	const deferredValue = useDeferredValue(value);
	const palette = createEditorPalette(theme);
	const prismConfig = getPrismLanguageConfig(language);
	const prismLanguage = normalizePrismLanguage(language);
	const resolvedPrismLanguage = hasPrismGrammar(prismLanguage)
		? prismLanguage
		: "text";
	const readOnly = options?.readOnly ?? options?.domReadOnly ?? false;
	const fontSize = options?.fontSize ?? 13;
	const lineHeight = Math.round(fontSize * 1.85);
	const lineCount = value.split("\n").length;
	const showLineNumbers = options?.lineNumbers !== "off";
	const gutterWidth = showLineNumbers
		? Math.max(44, String(lineCount).length * 10 + 20)
		: 0;
	const paddingTop = options?.padding?.top ?? 0;
	const paddingBottom =
		options?.scrollBeyondLastLine === false ? 16 : lineHeight * 4;
	const prismValue = readOnly ? value : deferredValue;
	const wrap = options?.wordWrap !== "off";

	useEffect(() => {
		onMountRef.current = onMount;
	}, [onMount]);

	useEffect(() => {
		let cancelled = false;

		if (hasPrismGrammar(prismConfig.grammar)) {
			return;
		}

		void ensurePrismLanguage(prismConfig).then(() => {
			if (!cancelled) {
				setPrismRevision((revision) => revision + 1);
			}
		});

		return () => {
			cancelled = true;
		};
	}, [prismConfig]);

	useEffect(() => {
		const commands = commandsRef.current;
		commands.clear();
		onMountRef.current?.(
			{
				addCommand(keybinding, handler) {
					commands.set(keybinding, handler);
				},
			},
			{
				KeyCode: KEY_CODE,
				KeyMod: KEY_MOD,
			},
		);

		return () => {
			commands.clear();
		};
	}, []);

	return (
		<div
			className="h-full w-full overflow-hidden"
			style={{ background: palette.background }}
		>
			{readOnly ? (
				<div className="h-full overflow-auto">
					<div
						className="grid min-h-full min-w-full"
						style={{
							gridTemplateColumns: showLineNumbers
								? `${gutterWidth}px minmax(0, 1fr)`
								: "minmax(0, 1fr)",
						}}
					>
						{showLineNumbers ? (
							<div
								className="border-r px-2 text-right select-none"
								style={{
									background: palette.gutterBackground,
									borderColor: palette.border,
									color: palette.gutterForeground,
									fontFamily: MONO_FONT_FAMILY,
									fontSize,
									paddingTop,
								}}
							>
								{renderLineNumbers(lineCount, lineHeight)}
							</div>
						) : null}
						<div className="min-w-0">
							<Highlight
								prism={Prism}
								theme={palette.theme}
								code={prismValue}
								language={resolvedPrismLanguage}
							>
								{({
									className,
									style,
									tokens,
									getLineProps,
									getTokenProps,
								}) => (
									<pre
										className={className}
										style={{
											...style,
											background: "transparent",
											fontFamily: MONO_FONT_FAMILY,
											fontSize,
											lineHeight: `${lineHeight}px`,
											margin: 0,
											minHeight: "100%",
											padding: `${paddingTop}px 16px ${paddingBottom}px 16px`,
											whiteSpace: wrap ? "pre-wrap" : "pre",
											wordBreak: wrap ? "break-word" : "normal",
										}}
									>
										{renderHighlightedLines({
											tokens,
											getLineProps,
											getTokenProps,
										})}
									</pre>
								)}
							</Highlight>
						</div>
					</div>
				</div>
			) : (
				<div className="relative h-full w-full overflow-hidden">
					{showLineNumbers ? (
						<div
							className="absolute top-0 bottom-0 left-0 overflow-hidden border-r px-2 text-right select-none"
							style={{
								background: palette.gutterBackground,
								borderColor: palette.border,
								color: palette.gutterForeground,
								fontFamily: MONO_FONT_FAMILY,
								fontSize,
								width: gutterWidth,
							}}
						>
							<div ref={gutterContentRef} style={{ paddingTop }}>
								{renderLineNumbers(lineCount, lineHeight)}
							</div>
						</div>
					) : null}
					<div
						className="absolute inset-0 overflow-hidden"
						style={{ left: gutterWidth }}
					>
						<div className="pointer-events-none absolute inset-0 overflow-hidden">
							<div ref={overlayContentRef}>
								<Highlight
									prism={Prism}
									theme={palette.theme}
									code={prismValue}
									language={resolvedPrismLanguage}
								>
									{({
										className,
										style,
										tokens,
										getLineProps,
										getTokenProps,
									}) => (
										<pre
											aria-hidden="true"
											className={className}
											style={{
												...style,
												background: "transparent",
												fontFamily: MONO_FONT_FAMILY,
												fontSize,
												lineHeight: `${lineHeight}px`,
												margin: 0,
												minHeight: "100%",
												padding: `${paddingTop}px 16px ${paddingBottom}px 16px`,
												whiteSpace: wrap ? "pre-wrap" : "pre",
												wordBreak: wrap ? "break-word" : "normal",
											}}
										>
											{renderHighlightedLines({
												tokens,
												getLineProps,
												getTokenProps,
											})}
										</pre>
									)}
								</Highlight>
							</div>
						</div>
						<textarea
							ref={textareaRef}
							aria-label="Code editor"
							autoCapitalize="off"
							autoCorrect="off"
							className="code-preview-editor-input absolute inset-0 h-full w-full resize-none border-0 bg-transparent outline-none"
							spellCheck={false}
							wrap={wrap ? "soft" : "off"}
							value={value}
							onChange={(event) => onChange?.(event.currentTarget.value)}
							onCompositionStart={() => {
								textareaComposingRef.current = true;
							}}
							onCompositionEnd={(event) => {
								textareaComposingRef.current = false;
								textareaCompositionEndAtRef.current = Date.now();
								onChange?.(event.currentTarget.value);
							}}
							onBlur={() => {
								textareaComposingRef.current = false;
							}}
							onKeyDown={(event) => {
								if (
									textareaComposingRef.current ||
									isImeComposingKeyEvent(event, {
										lastCompositionEndAt: textareaCompositionEndAtRef.current,
									})
								) {
									return;
								}

								if (event.key === "Tab" && !event.shiftKey) {
									event.preventDefault();
									onChange?.(insertTabAtSelection(event.currentTarget));
									return;
								}

								const keybinding = getKeybindingFromEvent(event);
								const command = commandsRef.current.get(keybinding);

								if (!command) {
									return;
								}

								event.preventDefault();
								command();
							}}
							onScroll={(event) => {
								const { scrollLeft, scrollTop } = event.currentTarget;

								if (overlayContentRef.current) {
									overlayContentRef.current.style.transform = `translate(${-scrollLeft}px, ${-scrollTop}px)`;
								}

								if (gutterContentRef.current) {
									gutterContentRef.current.style.transform = `translateY(${-scrollTop}px)`;
								}
							}}
							style={{
								WebkitTextFillColor: "transparent",
								caretColor: palette.caret,
								color: "transparent",
								fontFamily: MONO_FONT_FAMILY,
								fontSize,
								lineHeight: `${lineHeight}px`,
								padding: `${paddingTop}px 16px ${paddingBottom}px 16px`,
								tabSize: 4,
								whiteSpace: wrap ? "pre-wrap" : "pre",
								wordBreak: wrap ? "break-word" : "normal",
							}}
						/>
						<style>{`.code-preview-editor-input::selection { background: ${palette.selection}; }`}</style>
					</div>
				</div>
			)}
		</div>
	);
}

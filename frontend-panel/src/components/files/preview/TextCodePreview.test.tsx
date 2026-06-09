import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TextCodePreview } from "@/components/files/preview/TextCodePreview";

const mockState = vi.hoisted(() => ({
	cancelEditing: vi.fn(),
	editorProps: null as null | Record<string, unknown>,
	getEditorLanguage: vi.fn(),
	reload: vi.fn(),
	registeredShortcut: null as null | (() => void),
	save: vi.fn(),
	setEditContent: vi.fn(),
	startEditing: vi.fn(),
	useFileEditorSession: vi.fn(),
	useTextContent: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^(core|files):/, ""),
	}),
}));

vi.mock("@/components/files/preview/CodePreviewEditor", () => ({
	CodePreviewEditor: (props: {
		language: string;
		onChange?: (value: string) => void;
		onMount?: (
			editor: {
				addCommand: (keybinding: number, handler: () => void) => void;
			},
			shortcutApi: {
				KeyCode: { KeyS: number };
				KeyMod: { CtrlCmd: number };
			},
		) => void;
		options: { readOnly: boolean };
		theme: string;
		value: string;
	}) => {
		mockState.editorProps = props;
		props.onMount?.(
			{
				addCommand: (keybinding, handler) => {
					mockState.registeredShortcut = handler;
					mockState.save.mockName(`shortcut:${keybinding}`);
				},
			},
			{
				KeyCode: { KeyS: 49 },
				KeyMod: { CtrlCmd: 2048 },
			},
		);

		return (
			<div>
				<div data-testid="editor">{`editor:${props.language}:${props.theme}:${String(props.options.readOnly)}:${props.value}`}</div>
				<button type="button" onClick={() => props.onChange?.("next draft")}>
					change-editor
				</button>
			</div>
		);
	},
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{`icon:${name}`}</span>,
}));

vi.mock("@/hooks/useFileEditorSession", () => ({
	useFileEditorSession: (...args: unknown[]) =>
		mockState.useFileEditorSession(...args),
}));

vi.mock("@/hooks/useTextContent", () => ({
	useTextContent: (...args: unknown[]) => mockState.useTextContent(...args),
}));

vi.mock("@/components/files/preview/file-capabilities", () => ({
	getEditorLanguage: (...args: unknown[]) =>
		mockState.getEditorLanguage(...args),
}));

const file = {
	id: 7,
	name: "notes.ts",
	mime_type: "text/typescript",
};

function createSessionState(
	overrides: Partial<ReturnType<typeof mockState.useFileEditorSession>> = {},
) {
	return {
		editing: false,
		dirty: false,
		editContent: "draft content",
		saving: false,
		setEditContent: mockState.setEditContent,
		startEditing: mockState.startEditing,
		cancelEditing: mockState.cancelEditing,
		save: mockState.save,
		...overrides,
	};
}

describe("TextCodePreview", () => {
	beforeEach(() => {
		mockState.cancelEditing.mockReset();
		mockState.editorProps = null;
		mockState.getEditorLanguage.mockReset();
		mockState.reload.mockReset();
		mockState.registeredShortcut = null;
		mockState.save.mockReset();
		mockState.setEditContent.mockReset();
		mockState.startEditing.mockReset();
		mockState.useFileEditorSession.mockReset();
		mockState.useTextContent.mockReset();
		mockState.getEditorLanguage.mockReturnValue("typescript");
		mockState.useTextContent.mockReturnValue({
			content: "const value = 1;",
			etag: '"etag-1"',
			loading: false,
			error: false,
			reload: mockState.reload,
		});
		mockState.useFileEditorSession.mockReturnValue(createSessionState());

		Object.defineProperty(window, "MutationObserver", {
			configurable: true,
			value: class MutationObserverMock {
				disconnect() {}
				observe() {}
			},
		});
	});

	it("shows a loading message while content is being fetched", () => {
		mockState.useTextContent.mockReturnValue({
			content: null,
			etag: null,
			loading: true,
			error: false,
			reload: mockState.reload,
		});

		render(<TextCodePreview file={file} path="/files/7/content" />);

		expect(screen.getByText("loading_preview")).toBeInTheDocument();
	});

	it("renders a retry state when content loading fails", () => {
		mockState.useTextContent.mockReturnValue({
			content: null,
			etag: null,
			loading: false,
			error: true,
			reload: mockState.reload,
		});

		render(<TextCodePreview file={file} path="/files/7/content" />);

		fireEvent.click(screen.getByRole("button", { name: /preview_retry/ }));

		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		expect(mockState.reload).toHaveBeenCalledTimes(1);
	});

	it("renders a read-only editor and starts editing when requested", () => {
		const onDirtyChange = vi.fn();

		render(
			<TextCodePreview
				file={file}
				path="/files/7/content"
				onDirtyChange={onDirtyChange}
			/>,
		);

		expect(mockState.useTextContent).toHaveBeenCalledWith("/files/7/content");
		expect(mockState.useFileEditorSession).toHaveBeenCalledWith(
			expect.objectContaining({
				fileId: 7,
				initialContent: "const value = 1;",
				etag: '"etag-1"',
				messages: {
					saved: "file_saved",
					editedByOthers: "edited_by_others",
				},
			}),
		);
		expect(screen.queryByText("notes.ts")).not.toBeInTheDocument();
		expect(screen.getByText("typescript")).toBeInTheDocument();
		expect(screen.getByText("open_with_code")).toBeInTheDocument();
		expect(screen.getByText("active")).toBeInTheDocument();
		expect(screen.getByTestId("editor")).toHaveTextContent(
			"editor:typescript:vs:true:const value = 1;",
		);
		expect(mockState.editorProps).toMatchObject({
			options: expect.objectContaining({
				readOnly: true,
				wordWrap: "off",
			}),
		});

		fireEvent.click(screen.getByText("edit"));

		expect(mockState.startEditing).toHaveBeenCalledTimes(1);
		expect(onDirtyChange).toHaveBeenCalledWith(false);
	});

	it("renders the editing state, updates content, and wires save actions", () => {
		document.documentElement.className = "dark";
		const onDirtyChange = vi.fn();
		mockState.useFileEditorSession.mockReturnValue(
			createSessionState({
				editing: true,
				dirty: true,
				editContent: "draft content",
			}),
		);

		render(
			<TextCodePreview
				file={file}
				path="/files/7/content"
				onDirtyChange={onDirtyChange}
			/>,
		);

		expect(screen.getByText("unsaved_changes")).toBeInTheDocument();
		expect(screen.getByText("save_shortcut_hint")).toBeInTheDocument();
		expect(screen.getByTestId("editor")).toHaveTextContent(
			"editor:typescript:vs-dark:false:draft content",
		);
		expect(mockState.editorProps).toMatchObject({
			options: expect.objectContaining({
				readOnly: false,
				wordWrap: "off",
			}),
		});

		fireEvent.click(screen.getByText("save"));
		fireEvent.click(screen.getByText("cancel"));
		fireEvent.click(screen.getByText("change-editor"));
		act(() => {
			mockState.registeredShortcut?.();
		});

		expect(mockState.save).toHaveBeenCalledTimes(2);
		expect(mockState.cancelEditing).toHaveBeenCalledTimes(1);
		expect(mockState.setEditContent).toHaveBeenCalledWith("next draft");
		expect(onDirtyChange).toHaveBeenCalledWith(true);
	});

	it("hides edit controls when the preview is read-only", () => {
		render(
			<TextCodePreview file={file} path="/files/7/content" editable={false} />,
		);

		expect(screen.queryByText("edit")).not.toBeInTheDocument();
		expect(screen.getByText("open_with_code")).toBeInTheDocument();
	});

	it("reloads and notifies after saves and conflicts", async () => {
		const onFileUpdated = vi.fn().mockResolvedValue(undefined);

		render(
			<TextCodePreview
				file={file}
				path="/files/7/content"
				onFileUpdated={onFileUpdated}
			/>,
		);

		const options = mockState.useFileEditorSession.mock.calls[0]?.[0] as {
			onConflict: () => void;
			onSaved: () => Promise<void>;
		};

		await act(async () => {
			await options.onSaved();
		});
		act(() => {
			options.onConflict();
		});

		expect(mockState.reload).toHaveBeenCalledTimes(2);
		expect(onFileUpdated).toHaveBeenCalledTimes(1);
	});
});

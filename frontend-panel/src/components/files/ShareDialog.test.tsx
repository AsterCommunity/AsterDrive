import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ShareDialog } from "@/components/files/ShareDialog";

const mockState = vi.hoisted(() => ({
	create: vi.fn(),
	getDirectLinkToken: vi.fn(),
	directUrl: vi.fn(),
	forceDownloadUrl: vi.fn(),
	handleApiError: vi.fn(),
	pageUrl: vi.fn(),
	toastSuccess: vi.fn(),
	clipboardWriteText: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, opts?: Record<string, unknown>) => {
			if (key === "share:share_dialog_title") {
				return `share:${opts?.name}`;
			}
			return key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/shareService", () => ({
	shareService: {
		create: (...args: unknown[]) => mockState.create(...args),
		pageUrl: (...args: unknown[]) => mockState.pageUrl(...args),
	},
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		getDirectLinkToken: (...args: unknown[]) =>
			mockState.getDirectLinkToken(...args),
		directUrl: (...args: unknown[]) => mockState.directUrl(...args),
		forceDownloadUrl: (...args: unknown[]) =>
			mockState.forceDownloadUrl(...args),
	},
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		type,
		disabled,
		form,
		onClick,
		className,
	}: {
		children: React.ReactNode;
		type?: "button" | "submit";
		disabled?: boolean;
		form?: string;
		onClick?: () => void;
		className?: string;
	}) => (
		<button
			type={type ?? "button"}
			disabled={disabled}
			form={form}
			onClick={onClick}
			className={className}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({
		children,
		open,
		onOpenChange,
		onOpenChangeComplete,
	}: {
		children: React.ReactNode;
		open: boolean;
		onOpenChange: (open: boolean) => void;
		onOpenChangeComplete?: (open: boolean) => void;
	}) =>
		open ? (
			<div data-testid="dialog">
				<button type="button" onClick={() => onOpenChange(false)}>
					close-dialog
				</button>
				<button type="button" onClick={() => onOpenChangeComplete?.(false)}>
					close-dialog-complete
				</button>
				{children}
			</div>
		) : null,
	DialogContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <h2 className={className}>{children}</h2>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		className,
		...props
	}: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input className={className} {...props} />
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
	}: {
		children: React.ReactNode;
		htmlFor?: string;
	}) => <label htmlFor={htmlFor}>{children}</label>,
}));

vi.mock("@/components/ui/select", async () => {
	const React = await import("react");

	function collectOptions(children: React.ReactNode): Array<{
		label: React.ReactNode;
		value: string;
	}> {
		const options: Array<{ label: React.ReactNode; value: string }> = [];

		React.Children.forEach(children, (child) => {
			if (!React.isValidElement(child)) {
				return;
			}

			if (typeof child.props.value === "string") {
				options.push({
					label: child.props.children,
					value: child.props.value,
				});
			}

			if (child.props.children) {
				options.push(...collectOptions(child.props.children));
			}
		});

		return options;
	}

	return {
		Select: ({
			children,
			value,
			onValueChange,
		}: {
			children: React.ReactNode;
			value?: string;
			onValueChange?: (value: string) => void;
		}) => (
			<select
				data-testid="share-expiry"
				value={value}
				onChange={(event) => onValueChange?.(event.target.value)}
			>
				{collectOptions(children).map((option) => (
					<option key={option.value} value={option.value}>
						{option.label}
					</option>
				))}
			</select>
		),
		SelectTrigger: ({ children }: { children?: React.ReactNode }) => (
			<>{children}</>
		),
		SelectValue: () => null,
		SelectContent: ({ children }: { children?: React.ReactNode }) => (
			<>{children}</>
		),
		SelectItem: ({
			children,
			value,
		}: {
			children: React.ReactNode;
			value: string;
		}) => <option value={value}>{children}</option>,
	};
});

describe("ShareDialog", () => {
	beforeEach(() => {
		mockState.create.mockReset();
		mockState.getDirectLinkToken.mockReset();
		mockState.directUrl.mockReset();
		mockState.forceDownloadUrl.mockReset();
		mockState.handleApiError.mockReset();
		mockState.pageUrl.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.clipboardWriteText.mockReset();
		mockState.pageUrl.mockImplementation(
			(token: string) => `${window.location.origin}/s/${token}`,
		);
		mockState.directUrl.mockImplementation(
			(token: string, fileName: string) =>
				`${window.location.origin}/d/${token}/${encodeURIComponent(fileName)}`,
		);
		mockState.forceDownloadUrl.mockImplementation(
			(token: string, fileName: string) =>
				`${window.location.origin}/d/${token}/${encodeURIComponent(fileName)}?download=1`,
		);

		Object.defineProperty(window.navigator, "clipboard", {
			value: {
				writeText: mockState.clipboardWriteText,
			},
			configurable: true,
		});
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("creates a share link with computed expiry and coerces invalid download limits", async () => {
		mockState.create.mockResolvedValue({ token: "public-token" });
		const beforeSubmit = Date.now();

		render(
			<ShareDialog open onOpenChange={vi.fn()} fileId={42} name="report.pdf" />,
		);

		const passwordInput = screen.getByLabelText(
			"share:share_password_optional",
		);
		expect(passwordInput).toHaveAttribute("autocomplete", "new-password");
		fireEvent.change(passwordInput, {
			target: { value: "secret" },
		});
		fireEvent.change(screen.getByTestId("share-expiry"), {
			target: { value: "1d" },
		});
		fireEvent.change(screen.getByLabelText("share:share_download_limit"), {
			target: { value: "abc" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "share:share_create_button" }),
		);

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledTimes(1);
		});
		expect(mockState.create).toHaveBeenCalledWith(
			expect.objectContaining({
				target: { type: "file", id: 42 },
				max_downloads: 0,
				password: "secret",
			}),
		);
		const expiresAt = mockState.create.mock.calls[0]?.[0]?.expires_at;
		expect(expiresAt).toEqual(expect.any(String));
		expect(new Date(expiresAt).getTime() - beforeSubmit).toBeGreaterThan(
			23 * 60 * 60 * 1000,
		);
		expect(new Date(expiresAt).getTime() - beforeSubmit).toBeLessThan(
			25 * 60 * 60 * 1000,
		);
		expect(
			await screen.findByDisplayValue(
				`${window.location.origin}/s/public-token`,
			),
		).toBeInTheDocument();
		expect(mockState.toastSuccess).toHaveBeenCalledWith("share:share_created");
	});

	it("keeps long share target names inside the dialog title", () => {
		const longName = `${"e28633d9605db1b835fbee64aa065e9c".repeat(4)}.jpg`;

		render(
			<ShareDialog open onOpenChange={vi.fn()} fileId={42} name={longName} />,
		);

		const titleText = screen.getByText(`share:${longName}`);
		const title = titleText.parentElement;

		expect(title).toHaveClass("min-w-0", "leading-snug");
		expect(titleText).toHaveClass("min-w-0", "break-words");
	});

	it("creates direct links for files and exposes a force-download variant", async () => {
		mockState.getDirectLinkToken.mockResolvedValue({ token: "direct-token" });

		render(
			<ShareDialog
				open
				onOpenChange={vi.fn()}
				fileId={13}
				name="stream.m3u8"
				initialMode="direct"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "share:share_create_button" }),
		);

		await waitFor(() => {
			expect(mockState.getDirectLinkToken).toHaveBeenCalledWith(13);
		});
		expect(
			await screen.findByDisplayValue(
				`${window.location.origin}/d/direct-token/stream.m3u8`,
			),
		).toBeInTheDocument();
		expect(
			screen.getByDisplayValue(
				`${window.location.origin}/d/direct-token/stream.m3u8?download=1`,
			),
		).toBeInTheDocument();
		expect(mockState.toastSuccess).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("share:share_created");

		fireEvent.click(screen.getByRole("button", { name: "share:share_done" }));

		expect(mockState.getDirectLinkToken).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledTimes(1);
	});

	it("tracks direct-link copy feedback per button", async () => {
		mockState.getDirectLinkToken.mockResolvedValue({ token: "direct-token" });
		mockState.clipboardWriteText.mockResolvedValue(undefined);

		render(
			<ShareDialog
				open
				onOpenChange={vi.fn()}
				fileId={13}
				name="stream.m3u8"
				initialMode="direct"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "share:share_create_button" }),
		);

		const primaryUrl = `${window.location.origin}/d/direct-token/stream.m3u8`;
		const forceDownloadUrl = `${primaryUrl}?download=1`;
		const primaryInput = await screen.findByDisplayValue(primaryUrl);
		const forceDownloadInput = screen.getByDisplayValue(forceDownloadUrl);
		const primaryCopyButton = primaryInput.nextElementSibling as HTMLElement;
		const forceDownloadCopyButton =
			forceDownloadInput.nextElementSibling as HTMLElement;

		vi.useFakeTimers();
		await act(async () => {
			fireEvent.click(primaryCopyButton);
			await Promise.resolve();
		});

		expect(mockState.clipboardWriteText).toHaveBeenCalledWith(primaryUrl);
		expect(within(primaryCopyButton).getByText("Check")).toBeInTheDocument();
		expect(
			within(forceDownloadCopyButton).getByText("Copy"),
		).toBeInTheDocument();

		await act(async () => {
			await vi.advanceTimersByTimeAsync(1000);
			fireEvent.click(forceDownloadCopyButton);
			await Promise.resolve();
		});

		expect(mockState.clipboardWriteText).toHaveBeenCalledWith(forceDownloadUrl);
		expect(within(primaryCopyButton).getByText("Copy")).toBeInTheDocument();
		expect(
			within(forceDownloadCopyButton).getByText("Check"),
		).toBeInTheDocument();

		await act(async () => {
			await vi.advanceTimersByTimeAsync(1000);
		});

		expect(within(primaryCopyButton).getByText("Copy")).toBeInTheDocument();
		expect(
			within(forceDownloadCopyButton).getByText("Check"),
		).toBeInTheDocument();
	});

	it("keeps long direct-link target names inside the dialog title", () => {
		const longName = `${"e28633d9605db1b835fbee64aa065e9c".repeat(4)}.jpg`;

		render(
			<ShareDialog
				open
				onOpenChange={vi.fn()}
				fileId={13}
				name={longName}
				initialMode="direct"
			/>,
		);

		const titleText = screen.getByText(`share:${longName}`);
		const title = titleText.parentElement;

		expect(title).toHaveClass("min-w-0", "leading-snug");
		expect(titleText).toHaveClass("min-w-0", "break-words");
	});

	it("honors the initial direct mode from the entry point", async () => {
		render(
			<ShareDialog
				open
				onOpenChange={vi.fn()}
				fileId={13}
				name="stream.m3u8"
				initialMode="direct"
			/>,
		);

		expect(
			screen.queryByLabelText("share:share_password_optional"),
		).not.toBeInTheDocument();
		expect(screen.queryByTestId("share-expiry")).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("share:share_download_limit"),
		).not.toBeInTheDocument();
	});

	it("copies the created link and resets the dialog state when done", async () => {
		const onOpenChange = vi.fn();
		mockState.create.mockResolvedValue({ token: "copied-token" });
		mockState.clipboardWriteText.mockResolvedValue(undefined);

		render(
			<ShareDialog open onOpenChange={onOpenChange} folderId={7} name="Docs" />,
		);

		fireEvent.change(screen.getByLabelText("share:share_download_limit"), {
			target: { value: "5" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "share:share_create_button" }),
		);

		await screen.findByDisplayValue(`${window.location.origin}/s/copied-token`);
		fireEvent.click(screen.getByRole("button", { name: "Copy" }));

		await waitFor(() => {
			expect(mockState.clipboardWriteText).toHaveBeenCalledWith(
				`${window.location.origin}/s/copied-token`,
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("copied_to_clipboard");

		fireEvent.click(screen.getByRole("button", { name: "share:share_done" }));

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(
			screen.getByDisplayValue(`${window.location.origin}/s/copied-token`),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "close-dialog-complete" }),
		);

		expect(
			screen.getByRole("button", { name: "share:share_create_button" }),
		).toBeInTheDocument();
		expect(
			screen.queryByDisplayValue(`${window.location.origin}/s/copied-token`),
		).not.toBeInTheDocument();
	});

	it("reports create failures through the api error handler", async () => {
		const error = new Error("boom");
		mockState.create.mockRejectedValue(error);

		render(
			<ShareDialog
				open
				onOpenChange={vi.fn()}
				folderId={9}
				name="Shared folder"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "share:share_create_button" }),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(mockState.toastSuccess).not.toHaveBeenCalledWith(
			"share:share_created",
		);
	});

	it("clears pending form state when the dialog closes externally", () => {
		const onOpenChange = vi.fn();

		render(
			<ShareDialog
				open
				onOpenChange={onOpenChange}
				fileId={1}
				name="draft.md"
			/>,
		);

		fireEvent.change(screen.getByLabelText("share:share_password_optional"), {
			target: { value: "keep-out" },
		});
		fireEvent.change(screen.getByTestId("share-expiry"), {
			target: { value: "7d" },
		});
		fireEvent.change(screen.getByLabelText("share:share_download_limit"), {
			target: { value: "8" },
		});
		fireEvent.click(screen.getByRole("button", { name: "close-dialog" }));

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(screen.getByLabelText("share:share_password_optional")).toHaveValue(
			"keep-out",
		);

		fireEvent.click(
			screen.getByRole("button", { name: "close-dialog-complete" }),
		);

		const dialog = screen.getByTestId("dialog");
		expect(
			within(dialog).getByLabelText("share:share_password_optional"),
		).toHaveValue("");
		expect(within(dialog).getByTestId("share-expiry")).toHaveValue("never");
		expect(
			within(dialog).getByLabelText("share:share_download_limit"),
		).toHaveValue(null);
	});
});

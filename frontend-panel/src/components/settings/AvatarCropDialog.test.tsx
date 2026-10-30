import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AvatarCropDialog } from "@/components/settings/AvatarCropDialog";
import { DialogTestRoot } from "@/test/DialogTestRoot";

const mockState = vi.hoisted(() => ({
	cropAvatarImage: vi.fn(),
	renderAvatarCropPreview: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options?.size ? `${key}:${options.size}` : key,
	}),
}));

vi.mock("react-image-crop", () => ({
	centerCrop: (crop: unknown) => ({
		...(crop as object),
		x: 18,
		y: 18,
	}),
	convertToPixelCrop: (crop: unknown) => ({
		...(crop as object),
		unit: "px",
	}),
	default: ({
		children,
		onChange,
	}: {
		children: ReactNode;
		onChange: (
			pixelCrop: {
				height: number;
				unit: "px";
				width: number;
				x: number;
				y: number;
			},
			percentCrop: {
				height: number;
				unit: "%";
				width: number;
				x: number;
				y: number;
			},
		) => void;
	}) => (
		<div data-testid="react-crop">
			<button
				type="button"
				onClick={() =>
					onChange(
						{ height: 120, unit: "px", width: 120, x: 10, y: 12 },
						{ height: 50, unit: "%", width: 50, x: 5, y: 6 },
					)
				}
			>
				mock-crop-change
			</button>
			{children}
		</div>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
		...props
	}: {
		children: ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button
			{...props}
			type={type ?? "button"}
			disabled={disabled}
			onClick={onClick}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: (props: React.ComponentProps<typeof DialogTestRoot>) => (
		<DialogTestRoot {...props} />
	),
	DialogContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	DialogDescription: ({ children }: { children: ReactNode }) => (
		<p>{children}</p>
	),
	DialogFooter: ({ children }: { children: ReactNode }) => (
		<footer>{children}</footer>
	),
	DialogHeader: ({ children }: { children: ReactNode }) => (
		<header>{children}</header>
	),
	DialogTitle: ({ children }: { children: ReactNode }) => <h2>{children}</h2>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/lib/avatarCrop", () => ({
	cropAvatarImage: (...args: unknown[]) => mockState.cropAvatarImage(...args),
	renderAvatarCropPreview: (...args: unknown[]) =>
		mockState.renderAvatarCropPreview(...args),
}));

describe("AvatarCropDialog", () => {
	it("retains its source URL until exit completes even when the parent clears the file", () => {
		const file = new File(["image"], "avatar.png", { type: "image/png" });
		const onOpenChange = vi.fn();
		const onConfirm = vi.fn();
		const { rerender } = render(
			<AvatarCropDialog
				open
				file={file}
				onOpenChange={onOpenChange}
				onConfirm={onConfirm}
			/>,
		);
		rerender(
			<AvatarCropDialog
				open={false}
				file={null}
				onOpenChange={onOpenChange}
				onConfirm={onConfirm}
			/>,
		);
		expect(document.querySelector("img")).toHaveAttribute(
			"src",
			"blob:avatar-source",
		);
		expect(URL.revokeObjectURL).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "complete-close" }));
		expect(document.querySelector("img")).toBeNull();
		expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:avatar-source");
	});

	beforeEach(() => {
		mockState.cropAvatarImage.mockReset();
		mockState.renderAvatarCropPreview.mockReset();
		mockState.cropAvatarImage.mockResolvedValue(
			new File(["cropped"], "avatar.webp", { type: "image/webp" }),
		);
		vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:avatar-source");
		vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
	});

	it("loads the selected file, previews crop changes, resets and confirms", async () => {
		const onConfirm = vi.fn().mockResolvedValue(true);
		const onOpenChange = vi.fn();

		render(
			<AvatarCropDialog
				open
				file={new File(["source"], "avatar.png", { type: "image/png" })}
				onConfirm={onConfirm}
				onOpenChange={onOpenChange}
			/>,
		);

		const image = document.querySelector("img");
		if (!(image instanceof HTMLImageElement)) {
			throw new Error("avatar image not found");
		}
		Object.defineProperties(image, {
			height: { configurable: true, value: 300 },
			width: { configurable: true, value: 400 },
		});
		fireEvent.load(image);

		await waitFor(() => {
			expect(mockState.renderAvatarCropPreview).toHaveBeenCalledWith(
				image,
				expect.any(HTMLCanvasElement),
				expect.objectContaining({ unit: "px" }),
				192,
			);
		});
		expect(
			screen.getByRole("button", { name: /settings_avatar_crop_apply/ }),
		).toBeEnabled();

		fireEvent.click(screen.getByRole("button", { name: "mock-crop-change" }));
		expect(mockState.renderAvatarCropPreview).toHaveBeenLastCalledWith(
			image,
			expect.any(HTMLCanvasElement),
			expect.objectContaining({ height: 120, width: 120, x: 10, y: 12 }),
			192,
		);

		fireEvent.click(
			screen.getByRole("button", {
				name: /settings_avatar_crop_reset/,
			}),
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: /settings_avatar_crop_apply/,
			}),
		);

		await waitFor(() => {
			expect(mockState.cropAvatarImage).toHaveBeenCalledWith(
				image,
				expect.any(File),
				expect.objectContaining({ unit: "px" }),
			);
		});
		expect(onConfirm).toHaveBeenCalledWith(expect.any(File));
		await waitFor(() => {
			expect(onOpenChange).toHaveBeenCalledWith(false);
		});
	});

	it("keeps the dialog open when confirm returns false", async () => {
		const onConfirm = vi.fn().mockResolvedValue(false);
		const onOpenChange = vi.fn();

		render(
			<AvatarCropDialog
				open
				file={new File(["source"], "avatar.png")}
				onConfirm={onConfirm}
				onOpenChange={onOpenChange}
			/>,
		);

		const image = document.querySelector("img");
		if (!(image instanceof HTMLImageElement)) {
			throw new Error("avatar image not found");
		}
		Object.defineProperties(image, {
			height: { configurable: true, value: 256 },
			width: { configurable: true, value: 256 },
		});
		fireEvent.load(image);
		fireEvent.click(
			screen.getByRole("button", {
				name: /settings_avatar_crop_apply/,
			}),
		);

		await waitFor(() => {
			expect(onConfirm).toHaveBeenCalled();
		});
		expect(onOpenChange).not.toHaveBeenCalledWith(false);
	});

	it("blocks closing and disables actions while busy", () => {
		const onOpenChange = vi.fn();

		render(
			<AvatarCropDialog
				open
				busy
				file={new File(["source"], "avatar.png")}
				onConfirm={vi.fn()}
				onOpenChange={onOpenChange}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "dialog-close" }));
		fireEvent.click(screen.getByRole("button", { name: "core:cancel" }));

		expect(onOpenChange).not.toHaveBeenCalled();
		expect(
			screen.getByRole("button", {
				name: /settings_avatar_crop_apply/,
			}),
		).toBeDisabled();
	});

	it("shows loading without a file and revokes object URLs when replaced", () => {
		const { rerender, unmount } = render(
			<AvatarCropDialog
				open
				file={null}
				onConfirm={vi.fn()}
				onOpenChange={vi.fn()}
			/>,
		);

		expect(screen.getByText("core:loading")).toBeInTheDocument();

		rerender(
			<AvatarCropDialog
				open
				file={new File(["source"], "avatar.png")}
				onConfirm={vi.fn()}
				onOpenChange={vi.fn()}
			/>,
		);
		unmount();

		expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:avatar-source");
	});
});

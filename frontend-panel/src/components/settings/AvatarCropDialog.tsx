import { type SyntheticEvent, useEffect, useRef, useState } from "react";
import ReactCrop, {
	centerCrop,
	convertToPixelCrop,
	type PercentCrop,
	type PixelCrop,
} from "react-image-crop";
import "react-image-crop/dist/ReactCrop.css";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { cropAvatarImage, renderAvatarCropPreview } from "@/lib/avatarCrop";

interface AvatarCropDialogProps {
	open: boolean;
	file: File | null;
	busy?: boolean;
	onOpenChange: (open: boolean) => void;
	onConfirm: (file: File) => Promise<boolean>;
}

const CROPPER_SIZE_PERCENT = 64;
const MIN_CROP_SIZE = 88;
const PREVIEW_SIZE = 192;

function createCenteredAvatarCrop(
	containerWidth: number,
	containerHeight: number,
): PercentCrop {
	const baseCrop =
		containerWidth <= containerHeight
			? {
					unit: "%" as const,
					width: CROPPER_SIZE_PERCENT,
					height: (containerWidth / containerHeight) * CROPPER_SIZE_PERCENT,
				}
			: {
					unit: "%" as const,
					width: (containerHeight / containerWidth) * CROPPER_SIZE_PERCENT,
					height: CROPPER_SIZE_PERCENT,
				};

	return centerCrop(
		{
			...baseCrop,
			x: 0,
			y: 0,
		},
		containerWidth,
		containerHeight,
	);
}

export function AvatarCropDialog({
	open,
	file,
	busy = false,
	onOpenChange,
	onConfirm,
}: AvatarCropDialogProps) {
	const { t } = useTranslation(["core", "settings"]);
	const imageRef = useRef<HTMLImageElement | null>(null);
	const previewCanvasRef = useRef<HTMLCanvasElement | null>(null);
	const [crop, setCrop] = useState<PercentCrop>();
	const [completedCrop, setCompletedCrop] = useState<PixelCrop | null>(null);
	const [imageUrl, setImageUrl] = useState<string | null>(null);
	const [processing, setProcessing] = useState(false);

	useEffect(() => {
		imageRef.current = null;
		setCrop(undefined);
		setCompletedCrop(null);

		if (!open || !file) {
			setImageUrl(null);
			return;
		}

		const objectUrl = URL.createObjectURL(file);
		setImageUrl(objectUrl);
		return () => {
			URL.revokeObjectURL(objectUrl);
		};
	}, [file, open]);

	useEffect(() => {
		if (
			!open ||
			!completedCrop ||
			!imageRef.current ||
			!previewCanvasRef.current
		) {
			return;
		}

		renderAvatarCropPreview(
			imageRef.current,
			previewCanvasRef.current,
			completedCrop,
			PREVIEW_SIZE,
		);
	}, [completedCrop, open]);

	const handleDialogOpenChange = (nextOpen: boolean) => {
		if ((busy || processing) && !nextOpen) {
			return;
		}
		onOpenChange(nextOpen);
	};

	const handleImageLoad = (event: SyntheticEvent<HTMLImageElement>) => {
		const image = event.currentTarget;
		imageRef.current = image;

		const nextCrop = createCenteredAvatarCrop(image.width, image.height);
		setCrop(nextCrop);
		setCompletedCrop(convertToPixelCrop(nextCrop, image.width, image.height));
	};

	const handleReset = () => {
		const image = imageRef.current;
		if (!image) {
			return;
		}

		const nextCrop = createCenteredAvatarCrop(image.width, image.height);
		setCrop(nextCrop);
		setCompletedCrop(convertToPixelCrop(nextCrop, image.width, image.height));
	};

	const handleConfirm = async () => {
		if (!file || !completedCrop || !imageRef.current) {
			return;
		}

		try {
			setProcessing(true);
			const croppedFile = await cropAvatarImage(
				imageRef.current,
				file,
				completedCrop,
			);
			const shouldClose = await onConfirm(croppedFile);
			if (shouldClose) {
				onOpenChange(false);
			}
		} finally {
			setProcessing(false);
		}
	};

	return (
		<Dialog open={open} onOpenChange={handleDialogOpenChange}>
			<DialogContent
				showCloseButton={!busy && !processing}
				className="flex max-h-[min(820px,calc(100vh-2rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(1040px,calc(100vw-2rem))]"
			>
				<DialogHeader className="shrink-0 border-b px-6 pt-5 pb-0 pr-14">
					<DialogTitle>{t("settings:settings_avatar_crop_title")}</DialogTitle>
					<DialogDescription>
						{t("settings:settings_avatar_crop_desc")}
					</DialogDescription>
				</DialogHeader>

				<div className="grid min-h-0 flex-1 gap-0 lg:grid-cols-[320px_minmax(0,1fr)]">
					<aside className="flex min-h-0 flex-col border-b bg-muted/15 lg:border-r lg:border-b-0">
						<div className="min-h-0 flex-1 space-y-5 overflow-auto p-6">
							<section className="rounded-3xl border bg-background p-5">
								<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
									{t("settings:settings_avatar_crop_preview")}
								</p>
								<div className="mt-5 flex justify-center">
									<canvas
										ref={previewCanvasRef}
										className="size-48 rounded-full bg-muted ring-1 ring-border/45"
										aria-label={t("settings:settings_avatar_crop_preview")}
									/>
								</div>
								<p className="mt-4 text-center text-xs text-muted-foreground">
									{t("settings:settings_avatar_crop_output_hint", {
										size: "1024x1024",
									})}
								</p>
							</section>

							<section className="rounded-3xl border bg-background p-5">
								<p className="text-sm font-semibold">
									{t("settings:settings_avatar_crop_guide_title")}
								</p>
								<p className="mt-2 text-sm leading-6 text-muted-foreground">
									{t("settings:settings_avatar_crop_hint")}
								</p>
							</section>
						</div>

						<div className="shrink-0 border-t px-6 py-4">
							<Button
								type="button"
								variant="outline"
								className="w-full"
								disabled={busy || processing || !crop}
								onClick={handleReset}
							>
								<Icon name="Undo" className="mr-1 size-4" />
								{t("settings:settings_avatar_crop_reset")}
							</Button>
						</div>
					</aside>

					<section className="flex min-h-0 flex-1 items-center justify-center overflow-auto bg-muted/10 p-6 md:p-8">
						{imageUrl ? (
							<ReactCrop
								crop={crop}
								aspect={1}
								circularCrop
								keepSelection
								minWidth={MIN_CROP_SIZE}
								minHeight={MIN_CROP_SIZE}
								className="max-w-full"
								onChange={(pixelCrop, percentCrop) => {
									setCrop(percentCrop);
									setCompletedCrop(pixelCrop);
								}}
							>
								<img
									src={imageUrl}
									alt=""
									draggable={false}
									onLoad={handleImageLoad}
									className="block h-auto max-h-[min(58vh,540px)] w-auto max-w-full select-none object-contain"
								/>
							</ReactCrop>
						) : (
							<div className="flex items-center justify-center text-sm text-muted-foreground">
								<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
								{t("core:loading")}
							</div>
						)}
					</section>
				</div>

				<DialogFooter className="mx-0 mb-0 w-full shrink-0 border-t bg-muted/10 px-6 py-4 sm:flex-row sm:items-center sm:justify-end">
					<Button
						type="button"
						variant="outline"
						disabled={busy || processing}
						onClick={() => onOpenChange(false)}
					>
						{t("core:cancel")}
					</Button>
					<Button
						type="button"
						disabled={busy || processing || !completedCrop}
						onClick={() => void handleConfirm()}
					>
						{busy || processing ? (
							<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
						) : (
							<Icon name="Check" className="mr-1 size-4" />
						)}
						{t("settings:settings_avatar_crop_apply")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

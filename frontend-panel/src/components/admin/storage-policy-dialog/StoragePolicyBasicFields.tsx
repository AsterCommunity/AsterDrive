import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { SharedFieldProps } from "./StoragePolicyFieldTypes";

export function PolicyNameField({
	error,
	form,
	onFieldChange,
	showCreateValidation = false,
	t,
}: SharedFieldProps & {
	error: string | null;
	showCreateValidation?: boolean;
}) {
	return (
		<div className="space-y-2">
			<Label htmlFor="name">{t("core:name")}</Label>
			<Input
				id="name"
				value={form.name}
				onChange={(e) => onFieldChange("name", e.target.value)}
				aria-invalid={showCreateValidation && error ? true : undefined}
				className={ADMIN_CONTROL_HEIGHT_CLASS}
				required
			/>
			{showCreateValidation && error ? (
				<p className="text-xs text-destructive">{error}</p>
			) : null}
		</div>
	);
}

export function PolicyBasePathField({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	return (
		<div className="space-y-2">
			<Label htmlFor="base_path">{t("base_path")}</Label>
			<Input
				id="base_path"
				value={form.base_path}
				onChange={(e) => onFieldChange("base_path", e.target.value)}
				className={ADMIN_CONTROL_HEIGHT_CLASS}
				placeholder={form.driver_type === "local" ? "./data" : "tenant/prefix"}
			/>
		</div>
	);
}

export function LocalContentDedupField({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	return (
		<div className="space-y-2 pt-1">
			<div className="flex items-center gap-2">
				<Switch
					id="content_dedup"
					checked={form.content_dedup}
					onCheckedChange={(value) => onFieldChange("content_dedup", value)}
				/>
				<Label htmlFor="content_dedup">{t("content_dedup")}</Label>
			</div>
			<p className="text-xs text-muted-foreground">
				{t("local_content_dedup_desc")}
			</p>
		</div>
	);
}

export function StorageNativeProcessingField({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	const thumbnailExtensionsValue = form.thumbnail_extensions.join(", ");
	const mediaMetadataExtensionsValue = (
		form.media_metadata_extensions ?? []
	).join(", ");

	return (
		<div className="space-y-2 rounded-xl border border-border/70 bg-muted/20 p-4">
			<div className="flex items-center gap-2">
				<Switch
					id="storage_native_processing_enabled"
					checked={form.storage_native_processing_enabled}
					onCheckedChange={(value) =>
						onFieldChange("storage_native_processing_enabled", value)
					}
				/>
				<Label htmlFor="storage_native_processing_enabled">
					{t("storage_native_processing_enabled")}
				</Label>
			</div>
			<p className="text-xs leading-5 text-muted-foreground">
				{t("storage_native_processing_enabled_desc")}
			</p>
			{form.storage_native_processing_enabled ? (
				<div className="space-y-4 pt-2">
					<div className="space-y-2">
						<Label htmlFor="storage_native_thumbnail_extensions">
							{t("storage_native_thumbnail_extensions")}
						</Label>
						<Input
							id="storage_native_thumbnail_extensions"
							value={thumbnailExtensionsValue}
							onChange={(e) =>
								onFieldChange(
									"thumbnail_extensions",
									e.target.value.split(",").map((value) => value.trim()),
								)
							}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							placeholder="jpg, jpeg, png, webp, gif"
						/>
						<p className="text-xs leading-5 text-muted-foreground">
							{t("storage_native_thumbnail_extensions_desc")}
						</p>
					</div>

					<div className="space-y-2 border-t border-border/70 pt-4">
						<div className="flex items-center gap-2">
							<Switch
								id="storage_native_media_metadata_enabled"
								checked={form.storage_native_media_metadata_enabled === true}
								onCheckedChange={(value) =>
									onFieldChange("storage_native_media_metadata_enabled", value)
								}
							/>
							<Label htmlFor="storage_native_media_metadata_enabled">
								{t("storage_native_media_metadata_enabled")}
							</Label>
						</div>
						<p className="text-xs leading-5 text-muted-foreground">
							{t("storage_native_media_metadata_enabled_desc")}
						</p>
						{form.storage_native_media_metadata_enabled ? (
							<div className="space-y-2 pt-1">
								<Label htmlFor="storage_native_media_metadata_extensions">
									{t("storage_native_media_metadata_extensions")}
								</Label>
								<Input
									id="storage_native_media_metadata_extensions"
									value={mediaMetadataExtensionsValue}
									onChange={(e) =>
										onFieldChange(
											"media_metadata_extensions",
											e.target.value.split(",").map((value) => value.trim()),
										)
									}
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									placeholder="mp4, mov, mp3, flac"
								/>
								<p className="text-xs leading-5 text-muted-foreground">
									{t("storage_native_media_metadata_extensions_desc")}
								</p>
							</div>
						) : null}
					</div>
				</div>
			) : null}
		</div>
	);
}

export function LimitsFields({ form, onFieldChange, t }: SharedFieldProps) {
	return (
		<>
			<div className="space-y-2">
				<Label htmlFor="max_file_size">{t("max_file_size")} (bytes)</Label>
				<Input
					id="max_file_size"
					type="number"
					value={form.max_file_size}
					onChange={(e) => onFieldChange("max_file_size", e.target.value)}
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					placeholder={`0 = ${t("core:unlimited").toLowerCase()}`}
				/>
			</div>

			<div className="space-y-2">
				<Label htmlFor="chunk_size">{t("chunk_size")}</Label>
				<Input
					id="chunk_size"
					type="number"
					value={form.chunk_size}
					onChange={(e) => onFieldChange("chunk_size", e.target.value)}
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					placeholder="5 = 5MB, 0 = single upload only"
				/>
				<p className="text-xs text-muted-foreground">{t("chunk_size_desc")}</p>
			</div>
		</>
	);
}

export function DefaultPolicyToggle({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	return (
		<div className="flex items-center gap-2">
			<Switch
				id="is_default"
				checked={form.is_default}
				onCheckedChange={(value) => onFieldChange("is_default", value)}
			/>
			<Label htmlFor="is_default">{t("set_as_default")}</Label>
		</div>
	);
}

import { useTranslation } from "react-i18next";
import { AdminNumberUnitInput } from "@/components/admin/AdminNumberUnitInput";
import { STORAGE_QUOTA_UNITS, type StorageQuotaUnit } from "@/lib/storageQuota";

interface AdminStorageQuotaInputProps {
	disabled?: boolean;
	errorMessage?: string | null;
	id: string;
	label: string;
	onUnitChange: (value: StorageQuotaUnit) => void;
	onValueChange: (value: string) => void;
	placeholder: string;
	unit: StorageQuotaUnit;
	value: string;
}

export function AdminStorageQuotaInput({
	disabled,
	errorMessage,
	id,
	label,
	onUnitChange,
	onValueChange,
	placeholder,
	unit,
	value,
}: AdminStorageQuotaInputProps) {
	const { t } = useTranslation("admin");

	return (
		<AdminNumberUnitInput
			id={id}
			label={label}
			value={value}
			unit={unit}
			units={STORAGE_QUOTA_UNITS}
			disabled={disabled}
			errorMessage={errorMessage}
			placeholder={placeholder}
			unitAriaLabel={t("settings_size_unit_label")}
			onValueChange={onValueChange}
			onUnitChange={onUnitChange}
		/>
	);
}

import type { ReactNode } from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";

interface TestConnectionButtonProps {
	disabled?: boolean;
	label?: ReactNode;
	onTest: () => Promise<boolean>;
}

export function TestConnectionButton({
	disabled = false,
	label,
	onTest,
}: TestConnectionButtonProps) {
	const { t } = useTranslation("admin");
	const [testing, setTesting] = useState(false);
	const [result, setResult] = useState<boolean | null>(null);

	const handleTest = async () => {
		setTesting(true);
		setResult(null);
		try {
			const passed = await onTest();
			setResult(passed);
		} finally {
			setTesting(false);
		}
	};

	return (
		<Button
			type="button"
			variant="outline"
			className={ADMIN_CONTROL_HEIGHT_CLASS}
			disabled={testing || disabled}
			onClick={handleTest}
		>
			{testing ? (
				<Icon name="Spinner" className="h-4 w-4 mr-1 animate-spin" />
			) : result === true ? (
				<Icon
					name="Check"
					className="h-4 w-4 mr-1 text-green-600 dark:text-green-400"
				/>
			) : (
				<Icon name="WifiHigh" className="h-4 w-4 mr-1" />
			)}
			{label ?? t("test_connection")}
		</Button>
	);
}

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import { Icon } from "@/components/ui/icon";
import { Label } from "@/components/ui/label";
import type { WebdavCredentials } from "./useWebdavAccountDialogState";
import { WebdavCopyField } from "./WebdavCopyField";

interface WebdavCredentialsDialogProps {
	connectionFailedLabel: string;
	connectionSuccessLabel: string;
	credentials: WebdavCredentials | null;
	description: string;
	onCopy: (value: string) => void;
	onOpenChange: (open: boolean) => void;
	onTest: () => void;
	open: boolean;
	passwordLabel: string;
	testConnectionLabel: string;
	testResult: boolean | null;
	testing: boolean;
	title: string;
	usernameLabel: string;
}

export function WebdavCredentialsDialog(props: WebdavCredentialsDialogProps) {
	return (
		<DialogLifecycle
			open={props.open}
			identity={props.credentials?.username}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<WebdavCredentialsDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function WebdavCredentialsDialogContent({
	connectionFailedLabel,
	connectionSuccessLabel,
	credentials,
	description,
	onCopy,
	onOpenChange,
	onTest,
	open,
	passwordLabel,
	testConnectionLabel,
	testResult,
	testing,
	title,
	usernameLabel,
}: WebdavCredentialsDialogProps) {
	if (!credentials) {
		return null;
	}

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-md">
				<DialogHeader>
					<DialogTitle>{title}</DialogTitle>
					<DialogDescription>{description}</DialogDescription>
				</DialogHeader>
				<div className="space-y-4 py-2">
					<div className="space-y-1.5">
						<Label>{usernameLabel}</Label>
						<WebdavCopyField
							value={credentials.username}
							onCopy={() => {
								if (open) onCopy(credentials.username);
							}}
						/>
					</div>
					<div className="space-y-1.5">
						<Label>{passwordLabel}</Label>
						<WebdavCopyField
							value={open ? credentials.password : "••••••••"}
							onCopy={() => {
								if (open) onCopy(credentials.password);
							}}
						/>
					</div>
					{testResult !== null ? (
						<Badge
							variant={testResult ? "secondary" : "destructive"}
							className={
								testResult
									? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
									: undefined
							}
						>
							{testResult ? connectionSuccessLabel : connectionFailedLabel}
						</Badge>
					) : null}
				</div>
				<DialogFooter>
					<Button
						type="button"
						variant="outline"
						onClick={onTest}
						disabled={testing}
					>
						{testing ? (
							<Icon name="Spinner" className="size-4 animate-spin" />
						) : (
							<Icon name="WifiHigh" className="size-4" />
						)}
						{testConnectionLabel}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";

interface WebdavCreateAccountDialogProps {
	autoGenerateLabel: string;
	createLabel: string;
	createTitle: string;
	creating: boolean;
	description: string;
	loadingLabel: string;
	newPassword: string;
	newUsername: string;
	noFoldersLabel: string;
	onCreate: () => void;
	onOpenChange: (open: boolean) => void;
	onPasswordChange: (password: string) => void;
	onRootFolderChange: (folderId: number | undefined) => void;
	onUsernameChange: (username: string) => void;
	open: boolean;
	passwordLabel: string;
	rootFolderId: number | undefined;
	rootFolderLabel: string;
	rootFolderOptions: ReadonlyArray<{ label: string; value: string }>;
	usernameLabel: string;
	usernamePlaceholder: string;
}

export function WebdavCreateAccountDialog(
	props: WebdavCreateAccountDialogProps,
) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<WebdavCreateAccountDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function WebdavCreateAccountDialogContent({
	autoGenerateLabel,
	createLabel,
	createTitle,
	creating,
	description,
	loadingLabel,
	newPassword,
	newUsername,
	noFoldersLabel,
	onCreate,
	onOpenChange,
	onPasswordChange,
	onRootFolderChange,
	onUsernameChange,
	open,
	passwordLabel,
	rootFolderId,
	rootFolderLabel,
	rootFolderOptions,
	usernameLabel,
	usernamePlaceholder,
}: WebdavCreateAccountDialogProps) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-md">
				<DialogHeader>
					<DialogTitle>{createTitle}</DialogTitle>
					<DialogDescription>{description}</DialogDescription>
				</DialogHeader>
				<div className="space-y-4 py-2">
					<div className="space-y-1.5">
						<Label htmlFor="webdav-username">{usernameLabel}</Label>
						<Input
							id="webdav-username"
							value={newUsername}
							onChange={(event) => onUsernameChange(event.target.value)}
							placeholder={usernamePlaceholder}
						/>
					</div>
					<div className="space-y-1.5">
						<Label htmlFor="webdav-password">{passwordLabel}</Label>
						<Input
							id="webdav-password"
							type="password"
							value={newPassword}
							onChange={(event) => onPasswordChange(event.target.value)}
							placeholder={autoGenerateLabel}
						/>
					</div>
					<div className="space-y-1.5">
						<Label htmlFor="webdav-root-folder">{rootFolderLabel}</Label>
						<Select
							items={rootFolderOptions}
							value={rootFolderId != null ? String(rootFolderId) : "__all__"}
							onValueChange={(value) =>
								onRootFolderChange(
									value === "__all__" ? undefined : Number(value),
								)
							}
						>
							<SelectTrigger id="webdav-root-folder">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{rootFolderOptions.map((option) => (
									<SelectItem key={option.value} value={option.value}>
										{option.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
						{rootFolderOptions.length === 1 ? (
							<p className="text-xs text-muted-foreground">{noFoldersLabel}</p>
						) : null}
					</div>
				</div>
				<DialogFooter>
					<Button
						type="button"
						onClick={onCreate}
						disabled={creating || !newUsername.trim()}
					>
						<Icon name={creating ? "Spinner" : "Plus"} className="size-4" />
						{creating ? loadingLabel : createLabel}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

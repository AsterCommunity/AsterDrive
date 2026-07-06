export interface FileBrowserBatchActionPolicy {
	allowCopyMove?: boolean;
	allowDelete?: boolean;
	allowTagManagement?: boolean;
}

export const FILE_BROWSER_BATCH_ACTION_POLICIES = {
	full: {},
	publicShare: {
		allowCopyMove: false,
		allowDelete: false,
		allowTagManagement: false,
	},
	virtual: {
		allowCopyMove: false,
	},
} as const satisfies Record<string, FileBrowserBatchActionPolicy>;

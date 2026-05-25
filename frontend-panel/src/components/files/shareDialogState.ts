interface CreatedShareLinks {
	forceDownloadUrl: string | null;
	primaryUrl: string;
}

export interface ShareDialogState {
	copied: boolean;
	createdLinks: CreatedShareLinks | null;
	expiry: string;
	loading: boolean;
	maxDownloads: string;
	password: string;
}

export type ShareDialogAction =
	| { type: "setPassword"; value: string }
	| { type: "setExpiry"; value: string }
	| { type: "setMaxDownloads"; value: string }
	| { type: "createStarted" }
	| { type: "createFinished" }
	| { type: "createSucceeded"; links: CreatedShareLinks }
	| { type: "copySucceeded" }
	| { type: "copyReset" }
	| { type: "reset" };

export const initialShareDialogState: ShareDialogState = {
	copied: false,
	createdLinks: null,
	expiry: "never",
	loading: false,
	maxDownloads: "",
	password: "",
};

export function shareDialogReducer(
	state: ShareDialogState,
	action: ShareDialogAction,
): ShareDialogState {
	switch (action.type) {
		case "setPassword":
			return { ...state, password: action.value };
		case "setExpiry":
			return { ...state, expiry: action.value };
		case "setMaxDownloads":
			return { ...state, maxDownloads: action.value };
		case "createStarted":
			return { ...state, loading: true };
		case "createFinished":
			return { ...state, loading: false };
		case "createSucceeded":
			return {
				...state,
				createdLinks: action.links,
			};
		case "copySucceeded":
			return { ...state, copied: true };
		case "copyReset":
			return { ...state, copied: false };
		case "reset":
			return initialShareDialogState;
	}
}

export type EditSharePasswordAction = "clear" | "keep" | "set";

export interface EditShareDialogFormState {
	expiresAt: string;
	loading: boolean;
	maxDownloads: string;
	password: string;
	passwordAction: EditSharePasswordAction;
}

export type EditShareDialogAction =
	| {
			type: "resetForShare";
			expiresAt: string;
			maxDownloads: string;
	  }
	| { type: "setPasswordAction"; value: EditSharePasswordAction }
	| { type: "setPassword"; value: string }
	| { type: "setExpiresAt"; value: string }
	| { type: "setMaxDownloads"; value: string }
	| { type: "saveStarted" }
	| { type: "saveFinished" };

export const initialEditShareDialogFormState: EditShareDialogFormState = {
	expiresAt: "",
	loading: false,
	maxDownloads: "0",
	password: "",
	passwordAction: "keep",
};

export function editShareDialogReducer(
	state: EditShareDialogFormState,
	action: EditShareDialogAction,
): EditShareDialogFormState {
	switch (action.type) {
		case "resetForShare":
			return {
				...initialEditShareDialogFormState,
				expiresAt: action.expiresAt,
				maxDownloads: action.maxDownloads,
			};
		case "setPasswordAction":
			return { ...state, passwordAction: action.value };
		case "setPassword":
			return { ...state, password: action.value };
		case "setExpiresAt":
			return { ...state, expiresAt: action.value };
		case "setMaxDownloads":
			return { ...state, maxDownloads: action.value };
		case "saveStarted":
			return { ...state, loading: true };
		case "saveFinished":
			return { ...state, loading: false };
	}
}

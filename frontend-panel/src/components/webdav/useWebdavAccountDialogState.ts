import { useReducer } from "react";

export interface WebdavCredentials {
	password: string;
	username: string;
}

interface WebdavAccountDialogState {
	createDialogOpen: boolean;
	credentialsDialogOpen: boolean;
	creating: boolean;
	newPassword: string;
	newUsername: string;
	selectedFolderId: number | undefined;
	showPassword: WebdavCredentials | null;
	testing: boolean;
	testResult: boolean | null;
}

type WebdavAccountDialogAction =
	| { type: "createOpenChanged"; open: boolean }
	| { type: "credentialsOpenChanged"; open: boolean }
	| { type: "creatingChanged"; creating: boolean }
	| { type: "usernameChanged"; username: string }
	| { type: "passwordChanged"; password: string }
	| { type: "folderChanged"; folderId: number | undefined }
	| { type: "created"; credentials: WebdavCredentials; finishCreate: boolean }
	| { type: "credentialsCleared" }
	| { type: "testingChanged"; testing: boolean }
	| { type: "testResultChanged"; result: boolean | null };

const initialWebdavAccountDialogState: WebdavAccountDialogState = {
	createDialogOpen: false,
	credentialsDialogOpen: false,
	creating: false,
	newPassword: "",
	newUsername: "",
	selectedFolderId: undefined,
	showPassword: null,
	testing: false,
	testResult: null,
};

function webdavAccountDialogReducer(
	state: WebdavAccountDialogState,
	action: WebdavAccountDialogAction,
): WebdavAccountDialogState {
	switch (action.type) {
		case "createOpenChanged":
			return {
				...state,
				createDialogOpen: action.open,
				creating: action.open ? false : state.creating,
			};
		case "credentialsOpenChanged":
			return {
				...state,
				credentialsDialogOpen: action.open,
				testing: action.open ? false : state.testing,
			};
		case "creatingChanged":
			return { ...state, creating: action.creating };
		case "usernameChanged":
			return { ...state, newUsername: action.username };
		case "passwordChanged":
			return { ...state, newPassword: action.password };
		case "folderChanged":
			return { ...state, selectedFolderId: action.folderId };
		case "created":
			return {
				...state,
				credentialsDialogOpen: true,
				...(action.finishCreate || !state.createDialogOpen
					? {
							createDialogOpen: false,
							creating: false,
							newPassword: "",
							newUsername: "",
							selectedFolderId: undefined,
						}
					: {}),
				showPassword: action.credentials,
				testing: false,
				testResult: null,
			};
		case "credentialsCleared":
			return { ...state, showPassword: null };
		case "testingChanged":
			return { ...state, testing: action.testing };
		case "testResultChanged":
			return { ...state, testResult: action.result };
	}
}

export function useWebdavAccountDialogState() {
	const [state, dispatch] = useReducer(
		webdavAccountDialogReducer,
		initialWebdavAccountDialogState,
	);

	return {
		...state,
		clearCredentials: () => dispatch({ type: "credentialsCleared" }),
		setCreateDialogOpen: (open: boolean) =>
			dispatch({ type: "createOpenChanged", open }),
		setCredentialsDialogOpen: (open: boolean) =>
			dispatch({ type: "credentialsOpenChanged", open }),
		setCreating: (creating: boolean) =>
			dispatch({ type: "creatingChanged", creating }),
		setNewPassword: (password: string) =>
			dispatch({ type: "passwordChanged", password }),
		setNewUsername: (username: string) =>
			dispatch({ type: "usernameChanged", username }),
		setSelectedFolderId: (folderId: number | undefined) =>
			dispatch({ type: "folderChanged", folderId }),
		setTestResult: (result: boolean | null) =>
			dispatch({ type: "testResultChanged", result }),
		setTesting: (testing: boolean) =>
			dispatch({ type: "testingChanged", testing }),
		showCreatedCredentials: (
			credentials: WebdavCredentials,
			finishCreate = true,
		) => dispatch({ type: "created", credentials, finishCreate }),
	};
}

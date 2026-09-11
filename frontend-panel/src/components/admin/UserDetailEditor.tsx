import { useCallback, useEffect, useState } from "react";
import { handleApiError } from "@/hooks/useApiError";
import {
	loadAdminPolicyGroupLookup,
	readAdminPolicyGroupLookup,
} from "@/lib/adminPolicyGroupLookup";
import type {
	StoragePolicyGroup,
	UpdateUserRequest,
	UserInfo,
} from "@/types/api";
import { UserDetailEditorBody } from "./user-detail/UserDetailEditorBody";
import { userDetailDraftKey } from "./user-detail/userDetailEditorState";

interface UserDetailEditorProps {
	onBack: () => void;
	onUpdate: (id: number, data: UpdateUserRequest) => Promise<UserInfo>;
	user: UserInfo;
}

export function UserDetailEditor({
	onBack,
	onUpdate,
	user,
}: UserDetailEditorProps) {
	const initialPolicyGroups = readAdminPolicyGroupLookup();
	const [policyGroups, setPolicyGroups] = useState<StoragePolicyGroup[]>(
		initialPolicyGroups ?? [],
	);
	const [policyGroupsLoading, setPolicyGroupsLoading] = useState(
		initialPolicyGroups == null,
	);

	const loadPolicyGroups = useCallback(
		async (options?: { force?: boolean }) => {
			try {
				const cachedPolicyGroups = readAdminPolicyGroupLookup();
				if (!options?.force && cachedPolicyGroups != null) {
					setPolicyGroups(cachedPolicyGroups);
					setPolicyGroupsLoading(false);
				} else {
					setPolicyGroupsLoading(true);
				}
				setPolicyGroups(await loadAdminPolicyGroupLookup(options));
			} catch (e) {
				handleApiError(e);
			} finally {
				setPolicyGroupsLoading(false);
			}
		},
		[],
	);

	useEffect(() => {
		void loadPolicyGroups();
	}, [loadPolicyGroups]);

	return (
		<UserDetailEditorBody
			key={userDetailDraftKey(user)}
			onBack={onBack}
			onRefreshPolicyGroups={() => loadPolicyGroups({ force: true })}
			onUpdate={onUpdate}
			policyGroups={policyGroups}
			policyGroupsLoading={policyGroupsLoading}
			user={user}
		/>
	);
}

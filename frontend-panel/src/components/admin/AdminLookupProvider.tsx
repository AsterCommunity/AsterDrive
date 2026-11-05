import { type ReactNode, useState } from "react";
import { AdminLookupContext } from "@/hooks/adminLookupContext";
import { createAdminLookupResource } from "@/services/adminLookupResource";

export function AdminLookupProvider({ children }: { children: ReactNode }) {
	const [resource] = useState(createAdminLookupResource);
	return <AdminLookupContext value={resource}>{children}</AdminLookupContext>;
}

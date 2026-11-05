import { createContext } from "react";
import type { createAdminLookupResource } from "@/services/adminLookupResource";

export const AdminLookupContext = createContext<ReturnType<
	typeof createAdminLookupResource
> | null>(null);

import { Navigate, useParams } from "react-router-dom";
import AdminExternalAuthPage from "./AdminExternalAuthPage";

export default function AdminExternalAuthDetailPage() {
	const { providerId } = useParams<{ providerId?: string }>();
	const parsedProviderId = Number(providerId);
	if (!Number.isSafeInteger(parsedProviderId) || parsedProviderId <= 0) {
		return <Navigate to="/admin/external-auth" replace />;
	}
	return (
		<AdminExternalAuthPage variant="detail" providerId={parsedProviderId} />
	);
}

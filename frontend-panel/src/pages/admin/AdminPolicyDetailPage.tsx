import { Navigate, useParams } from "react-router-dom";
import AdminPoliciesPage from "./AdminPoliciesPage";

export default function AdminPolicyDetailPage() {
	const { policyId } = useParams<{ policyId?: string }>();
	const parsedPolicyId = Number(policyId);
	if (!Number.isSafeInteger(parsedPolicyId) || parsedPolicyId <= 0) {
		return <Navigate to="/admin/policies" replace />;
	}
	return <AdminPoliciesPage variant="detail" detailPolicyId={parsedPolicyId} />;
}

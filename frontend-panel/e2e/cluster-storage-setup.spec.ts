import {
	configureInitialStorage,
	createInitialAdmin,
	hasUsers,
	type InitialStorageSetup,
} from "./support/auth";
import { expect, test } from "./support/test";

const clusterSetupEnabled = process.env.ASTER_E2E_CLUSTER_SETUP === "1";

function requiredEnvironment(name: string) {
	const value = process.env[name]?.trim();
	if (!value) {
		throw new Error(`${name} is required for the cluster storage setup E2E`);
	}
	return value;
}

test.describe("Cluster storage setup E2E", () => {
	test("hides Local, explains disabled OneDrive, and binds RustFS through S3", async ({
		page,
		request,
	}) => {
		test.skip(
			!clusterSetupEnabled,
			"runs only against the dedicated PostgreSQL, Redis, and RustFS cluster fixture",
		);
		const storage: InitialStorageSetup = {
			kind: "cluster-s3",
			endpoint: requiredEnvironment("ASTER_E2E_S3_ENDPOINT"),
			bucket: requiredEnvironment("ASTER_E2E_S3_BUCKET"),
			accessKey: requiredEnvironment("ASTER_E2E_S3_ACCESS_KEY"),
			secretKey: requiredEnvironment("ASTER_E2E_S3_SECRET_KEY"),
			basePath: "cluster-setup",
		};

		expect(await hasUsers(request)).toBe(false);
		await createInitialAdmin(page);
		await configureInitialStorage(page, storage);
		await expect(page).toHaveURL(/\/$/);
	});
});

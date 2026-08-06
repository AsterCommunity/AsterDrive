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
	// Initial setup mutates the shared database before storage configuration is
	// complete. Playwright retries reuse that database and therefore no longer
	// start from the no-user boundary this test is intended to prove.
	test.describe.configure({ retries: 0 });

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
			s3AccessKeyId: requiredEnvironment("ASTER_E2E_S3_ACCESS_KEY"),
			s3SecretAccessKey: requiredEnvironment("ASTER_E2E_S3_SECRET_KEY"),
			basePath: "cluster-setup",
		};

		expect(await hasUsers(request)).toBe(false);
		await createInitialAdmin(page);
		await configureInitialStorage(page, storage);
		await expect(page).toHaveURL(/\/$/);
	});
});

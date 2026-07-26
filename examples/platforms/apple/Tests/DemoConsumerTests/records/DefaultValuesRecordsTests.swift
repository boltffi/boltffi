import Demo
import XCTest

final class DefaultValuesRecordsTests: DemoTestCase {
    func testServiceConfigDefaults() {
        let implicitDefaults = ServiceConfig(name: "worker")
        XCTAssertEqual(
            implicitDefaults,
            ServiceConfig(
                name: "worker",
                retries: 3,
                region: "standard",
                endpoint: nil,
                backupEndpoint: "https://default"
            )
        )

        let customRetries = ServiceConfig(name: "worker", retries: 7)
        XCTAssertEqual(
            customRetries,
            ServiceConfig(
                name: "worker",
                retries: 7,
                region: "standard",
                endpoint: nil,
                backupEndpoint: "https://default"
            )
        )

        let explicitRegion = ServiceConfig(name: "worker", retries: 9, region: "eu-west")
        XCTAssertNil(explicitRegion.endpoint)
        XCTAssertEqual(explicitRegion.backupEndpoint, "https://default")

        let explicitEndpoint = ServiceConfig(
            name: "worker",
            retries: 9,
            region: "eu-west",
            endpoint: "https://edge"
        )
        XCTAssertEqual(explicitEndpoint.backupEndpoint, "https://default")

        let explicitBackupEndpoint = ServiceConfig(
            name: "worker",
            retries: 9,
            region: "eu-west",
            endpoint: "https://edge",
            backupEndpoint: "https://backup"
        )
        demoCase("case:records.default_values.service_config.should_roundtrip_value")
        XCTAssertEqual(echoServiceConfig(config: explicitBackupEndpoint), explicitBackupEndpoint)
        demoCase("case:records.default_values.service_config.should_describe_values")
        XCTAssertEqual(implicitDefaults.describe(), "worker:3:standard:none:https://default")
        XCTAssertEqual(customRetries.describe(), "worker:7:standard:none:https://default")
        XCTAssertEqual(explicitRegion.describe(), "worker:9:eu-west:none:https://default")
        XCTAssertEqual(explicitEndpoint.describe(), "worker:9:eu-west:https://edge:https://default")
        XCTAssertEqual(explicitBackupEndpoint.describe(), "worker:9:eu-west:https://edge:https://backup")
        demoCase("case:records.default_values.service_config.should_describe_with_prefix")
        XCTAssertEqual(explicitBackupEndpoint.describeWithPrefix(prefix: "cfg"), "cfg:worker:9:eu-west:https://edge:https://backup")

        demoCase("case:records.default_values.service_config.from_owned_name.should_return_config")
        XCTAssertEqual(ServiceConfig(fromOwnedName: "owned").describe(), "owned:3:standard:none:https://default")
        demoCase("case:records.default_values.service_config.from_borrowed_name.should_return_config")
        XCTAssertEqual(ServiceConfig(fromBorrowedName: "borrowed").describe(), "borrowed:3:standard:none:https://default")
        demoCase("case:records.default_values.service_config.from_string_ref_name.should_return_config")
        XCTAssertEqual(ServiceConfig(fromStringRefName: "stringref").describe(), "stringref:3:standard:none:https://default")
        demoCase("case:records.default_values.service_config.from_non_empty_name.should_return_config_for_non_empty_values")
        let validatedConfig = ServiceConfig(fromNonEmptyName: "optional", region: "eu-west")
        XCTAssertEqual(validatedConfig?.name, "optional")
        XCTAssertEqual(validatedConfig?.region, "eu-west")
        demoCase("case:records.default_values.service_config.from_non_empty_name.should_return_none_for_empty_values")
        XCTAssertNil(ServiceConfig(fromNonEmptyName: "", region: "eu-west"))
        XCTAssertNil(ServiceConfig(fromNonEmptyName: "optional", region: ""))
    }
}

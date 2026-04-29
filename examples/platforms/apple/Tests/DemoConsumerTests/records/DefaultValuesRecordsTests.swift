import Demo
import XCTest

final class DefaultValuesRecordsTests: XCTestCase {
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
        XCTAssertEqual(echoServiceConfig(config: explicitBackupEndpoint), explicitBackupEndpoint)
        XCTAssertEqual(implicitDefaults.describe(), "worker:3:standard:none:https://default")
        XCTAssertEqual(customRetries.describe(), "worker:7:standard:none:https://default")
        XCTAssertEqual(explicitRegion.describe(), "worker:9:eu-west:none:https://default")
        XCTAssertEqual(explicitEndpoint.describe(), "worker:9:eu-west:https://edge:https://default")
        XCTAssertEqual(explicitBackupEndpoint.describe(), "worker:9:eu-west:https://edge:https://backup")
        XCTAssertEqual(explicitBackupEndpoint.describeWithPrefix(prefix: "cfg"), "cfg:worker:9:eu-west:https://edge:https://backup")
    }
}

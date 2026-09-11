import XCTest
@testable import OatsRecorderPill

final class BridgeTests: XCTestCase {
    func testAbiVersion() {
        XCTAssertEqual(oatsPillAbiVersion(), 1)
    }
}

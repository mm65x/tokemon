import Foundation
import XCTest
@testable import TokemonMenuBar

final class DashboardLauncherTests: XCTestCase {
    func testBuildsDirectOpenArgumentsWithoutShellInterpolation() {
        let executable = URL(fileURLWithPath: "/tmp/path with spaces/tokemon")

        XCTAssertEqual(
            DashboardLauncher.commandArguments(for: executable),
            ["-a", "Terminal", "/tmp/path with spaces/tokemon", "--args", "top"]
        )
    }
}

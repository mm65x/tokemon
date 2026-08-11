import Foundation
import XCTest
@testable import TokemonMenuBar

final class StatusRunnerTests: XCTestCase {
    func testRunsAStatusResponseFromTheConfiguredExecutable() throws {
        let runner = StatusRunner(executableURL: URL(fileURLWithPath: "/usr/bin/printf"))
        let result = try runner.run(arguments: ["%s", statusJSON])

        XCTAssertEqual(result.report.schemaVersion, 1)
        XCTAssertEqual(result.report.totalTokens, 15)
        XCTAssertTrue(result.stderr.isEmpty)
    }

    func testReportsAProcessFailure() {
        let runner = StatusRunner(executableURL: URL(fileURLWithPath: "/usr/bin/false"))

        XCTAssertThrowsError(try runner.run(arguments: [])) { error in
            XCTAssertEqual(error as? StatusRunnerError, .nonZeroExit(1, ""))
        }
    }

    func testBoundsCollectedOutput() {
        let runner = StatusRunner(
            executableURL: URL(fileURLWithPath: "/usr/bin/printf"),
            outputLimit: 2
        )

        XCTAssertThrowsError(try runner.run(arguments: ["%s", "123"])) { error in
            XCTAssertEqual(error as? StatusRunnerError, .outputTooLarge)
        }
    }

    func testTerminatesACommandThatExceedsTheTimeout() {
        let runner = StatusRunner(
            executableURL: URL(fileURLWithPath: "/bin/sleep"),
            timeout: 0.05
        )

        XCTAssertThrowsError(try runner.run(arguments: ["1"])) { error in
            XCTAssertEqual(error as? StatusRunnerError, .timedOut)
        }
    }

    private var statusJSON: String {
        """
        {"schema_version":1,"generated_at":"2026-08-11T12:00:00Z","state":"populated","scope":{"frequency":"daily","since":"2026-08-11","until":"2026-08-11"},"providers":["local"],"summaries":[],"total_cost":0.25,"total_tokens":15,"total_requests":1,"capabilities":{"cost":true,"date_filters":true,"provider_filters":true,"periodic_summaries":true,"session_view":true}}
        """
    }
}

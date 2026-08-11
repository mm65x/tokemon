import Foundation
import XCTest
@testable import TokemonMenuBar

@MainActor
final class AppModelTests: XCTestCase {
    func testRefreshPublishesTheReportAndUsesSelectedScope() async throws {
        let runner = RecordingRunner(result: StatusCommandResult(report: report, stderr: ""))
        let fixedDate = ISO8601DateFormatter().date(from: "2026-08-12T15:30:00Z")!
        let model = AppModel(
            preferences: AppPreferences(metric: .combined, scope: .week),
            runner: runner,
            clock: { fixedDate }
        )

        model.refresh()
        try await waitUntil { model.state == .loaded }

        XCTAssertEqual(model.report, report)
        XCTAssertEqual(model.menuBarTitle, "15 tok · $0.25")
        XCTAssertEqual(
            runner.arguments,
            ["--frequency", "weekly", "--since", "2026-08-10", "--until", "2026-08-12"]
        )

        runner.fail(with: "temporary status failure")
        try await Task.sleep(nanoseconds: 20_000_000)
        model.refresh()
        try await waitUntil {
            if case .failed = model.state { return true }
            return false
        }
        XCTAssertEqual(model.report, report)
        XCTAssertEqual(model.statusMessage, "temporary status failure")
    }

    func testCostMetricExplainsWhenPricingIsUnavailable() async throws {
        let unavailableReport = StatusReport(
            schemaVersion: 1,
            generatedAt: report.generatedAt,
            state: "populated",
            scope: report.scope,
            providers: report.providers,
            summaries: report.summaries,
            totalCost: 0,
            totalTokens: report.totalTokens,
            totalRequests: report.totalRequests,
            capabilities: StatusCapabilities(
                cost: false,
                dateFilters: true,
                providerFilters: true,
                periodicSummaries: true,
                sessionView: true
            )
        )
        let runner = RecordingRunner(result: StatusCommandResult(report: unavailableReport, stderr: ""))
        let model = AppModel(
            preferences: AppPreferences(metric: .cost),
            runner: runner
        )

        model.refresh()
        try await waitUntil { model.state == .loaded }

        XCTAssertEqual(model.menuBarTitle, "Cost unavailable")
    }

    func testMissingExecutableProducesAnActionableError() async throws {
        let model = AppModel(preferences: AppPreferences(executablePath: "/does/not/exist"))

        model.refresh()
        try await waitUntil {
            if case .failed = model.state { return true }
            return false
        }

        XCTAssertEqual(
            model.statusMessage,
            "The configured tokemon executable was not found. Update its path in Preferences."
        )
    }

    func testNonExecutablePathGetsADistinctError() async throws {
        let model = AppModel(preferences: AppPreferences(executablePath: "/etc/hosts"))

        model.refresh()
        try await waitUntil {
            if case .failed = model.state { return true }
            return false
        }

        XCTAssertEqual(
            model.statusMessage,
            "The configured tokemon path is not executable. Update its path in Preferences."
        )
    }

    private func waitUntil(
        timeout: TimeInterval = 2,
        condition: @escaping @MainActor () -> Bool
    ) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if condition() { return }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTFail("Condition was not met before timeout")
    }

    private var report: StatusReport {
        StatusReport(
            schemaVersion: 1,
            generatedAt: "2026-08-12T15:30:00Z",
            state: "populated",
            scope: StatusScope(frequency: "weekly", since: "2026-08-10", until: "2026-08-12"),
            providers: ["local"],
            summaries: [],
            totalCost: 0.25,
            totalTokens: 15,
            totalRequests: 1,
            capabilities: StatusCapabilities(
                cost: true,
                dateFilters: true,
                providerFilters: true,
                periodicSummaries: true,
                sessionView: true
            )
        )
    }
}

private final class RecordingRunner: StatusRunning, @unchecked Sendable {
    private let lock = NSLock()
    private let result: StatusCommandResult
    private var recordedArguments: [String] = []
    private var failureMessage: String?

    init(result: StatusCommandResult) {
        self.result = result
    }

    var arguments: [String] {
        lock.lock()
        defer { lock.unlock() }
        return recordedArguments
    }

    func fail(with message: String) {
        lock.lock()
        failureMessage = message
        lock.unlock()
    }

    func runOffline(arguments: [String]) throws -> StatusCommandResult {
        lock.lock()
        recordedArguments = arguments
        let failure = failureMessage
        lock.unlock()
        if let failure {
            throw NSError(domain: "StatusRunnerTests", code: 1, userInfo: [NSLocalizedDescriptionKey: failure])
        }
        return result
    }
}

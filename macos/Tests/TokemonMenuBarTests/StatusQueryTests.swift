import Foundation
import XCTest
@testable import TokemonMenuBar

final class StatusQueryTests: XCTestCase {
    private let now = ISO8601DateFormatter().date(from: "2026-08-12T15:30:00Z")!

    func testTodayUsesTheUtcCalendarDay() {
        XCTAssertEqual(
            StatusQuery(scope: .today).arguments(now: now),
            ["--frequency", "daily", "--since", "2026-08-12", "--until", "2026-08-12"]
        )
    }

    func testWeekStartsOnMonday() {
        XCTAssertEqual(
            StatusQuery(scope: .week).arguments(now: now),
            ["--frequency", "weekly", "--since", "2026-08-10", "--until", "2026-08-12"]
        )
    }

    func testMonthStartsOnTheFirstDay() {
        XCTAssertEqual(
            StatusQuery(scope: .month).arguments(now: now),
            ["--frequency", "monthly", "--since", "2026-08-01", "--until", "2026-08-12"]
        )
    }

    func testAllTimeUsesMonthlyRollupsWithoutDateFilters() {
        XCTAssertEqual(StatusQuery(scope: .allTime).arguments(now: now), ["--frequency", "monthly"])
    }

    func testProviderFiltersUseRepeatableArguments() {
        let query = StatusQuery(scope: .today, providers: [" codex ", "gemini", "codex"])

        XCTAssertEqual(
            query.arguments(now: now),
            [
                "--frequency", "daily", "--since", "2026-08-12", "--until", "2026-08-12",
                "--provider", "codex", "--provider", "gemini"
            ]
        )
    }
}

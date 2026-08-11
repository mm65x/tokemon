import Foundation
import XCTest
@testable import TokemonMenuBar

final class AppPreferencesTests: XCTestCase {
    func testRoundTripsPreferencesThroughUserDefaults() {
        let defaults = UserDefaults(suiteName: "TokemonMenuBarTests-\(UUID().uuidString)")!
        let expected = AppPreferences(
            executablePath: "/usr/local/bin/tokemon",
            metric: .cost,
            scope: .month,
            refreshInterval: 30
        )

        expected.save(to: defaults)

        XCTAssertEqual(AppPreferences.load(from: defaults), expected)
    }

    func testClampsRefreshIntervalToSafeBounds() {
        XCTAssertEqual(
            AppPreferences(refreshInterval: 1).refreshInterval,
            AppPreferences.minimumRefreshInterval
        )
        XCTAssertEqual(
            AppPreferences(refreshInterval: .greatestFiniteMagnitude).refreshInterval,
            AppPreferences.maximumRefreshInterval
        )
    }
}

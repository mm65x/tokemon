import Foundation
import XCTest
@testable import TokemonMenuBar

final class StatusModelsTests: XCTestCase {
    func testDecodesPopulatedStatusAndIgnoresUnknownFields() throws {
        let data = Data(
            """
            {
              "schema_version": 1,
              "generated_at": "2026-08-11T12:00:00Z",
              "state": "populated",
              "scope": {"frequency": "daily", "since": "2026-08-11", "until": "2026-08-11"},
              "providers": ["local"],
              "summaries": [{
                "date": "2026-08-11",
                "label": "2026-08-11",
                "models": [{
                  "model": "model-a",
                  "raw_model": "model-a",
                  "provider": "local",
                  "input_tokens": 10,
                  "output_tokens": 5,
                  "cache_read_tokens": 0,
                  "cache_creation_tokens": 0,
                  "thinking_tokens": 0,
                  "cost_usd": 0.25,
                  "request_count": 1
                }],
                "total_input": 10,
                "total_output": 5,
                "total_thinking": 0,
                "total_cost": 0.25,
                "total_requests": 1,
                "future_field": true
              }],
              "total_cost": 0.25,
              "total_tokens": 15,
              "total_requests": 1,
              "capabilities": {
                "cost": true,
                "date_filters": true,
                "provider_filters": true,
                "periodic_summaries": true,
                "session_view": true
              },
              "future_field": "ignored"
            }
            """.utf8
        )

        let report = try StatusDecoder.decode(data)

        XCTAssertEqual(report.schemaVersion, 1)
        XCTAssertEqual(report.state, "populated")
        XCTAssertEqual(report.totalTokens, 15)
        XCTAssertEqual(report.summaries.count, 1)
        XCTAssertTrue(report.capabilities.cost)
    }

    func testRejectsFutureSchema() throws {
        let data = Data(
            """
            {
              "schema_version": 2,
              "generated_at": "2026-08-11T12:00:00Z",
              "state": "empty",
              "scope": {"frequency": "daily", "since": null, "until": null},
              "providers": [], "summaries": [], "total_cost": 0,
              "total_tokens": 0, "total_requests": 0,
              "capabilities": {
                "cost": false, "date_filters": true, "provider_filters": true,
                "periodic_summaries": true, "session_view": true
              }
            }
            """.utf8
        )

        XCTAssertThrowsError(try StatusDecoder.decode(data)) { error in
            XCTAssertEqual(error as? StatusDecoderError, .unsupportedSchema(2))
        }
    }
}

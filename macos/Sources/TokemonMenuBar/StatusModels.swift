import Foundation

public struct StatusReport: Codable, Equatable, Sendable {
    public let schemaVersion: UInt32
    public let generatedAt: String
    public let state: String
    public let scope: StatusScope
    public let providers: [String]
    public let summaries: [PeriodSummary]
    public let totalCost: Double
    public let totalTokens: UInt64
    public let totalRequests: UInt64
    public let capabilities: StatusCapabilities

    public init(
        schemaVersion: UInt32,
        generatedAt: String,
        state: String,
        scope: StatusScope,
        providers: [String],
        summaries: [PeriodSummary],
        totalCost: Double,
        totalTokens: UInt64,
        totalRequests: UInt64,
        capabilities: StatusCapabilities
    ) {
        self.schemaVersion = schemaVersion
        self.generatedAt = generatedAt
        self.state = state
        self.scope = scope
        self.providers = providers
        self.summaries = summaries
        self.totalCost = totalCost
        self.totalTokens = totalTokens
        self.totalRequests = totalRequests
        self.capabilities = capabilities
    }

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case generatedAt = "generated_at"
        case state
        case scope
        case providers
        case summaries
        case totalCost = "total_cost"
        case totalTokens = "total_tokens"
        case totalRequests = "total_requests"
        case capabilities
    }
}

public struct StatusScope: Codable, Equatable, Sendable {
    public let frequency: String
    public let since: String?
    public let until: String?
}

public struct StatusCapabilities: Codable, Equatable, Sendable {
    public let cost: Bool
    public let dateFilters: Bool
    public let providerFilters: Bool
    public let periodicSummaries: Bool
    public let sessionView: Bool

    enum CodingKeys: String, CodingKey {
        case cost
        case dateFilters = "date_filters"
        case providerFilters = "provider_filters"
        case periodicSummaries = "periodic_summaries"
        case sessionView = "session_view"
    }
}

public struct PeriodSummary: Codable, Equatable, Sendable {
    public let date: String
    public let label: String
    public let models: [ModelUsage]
    public let totalInput: UInt64
    public let totalOutput: UInt64
    public let totalThinking: UInt64
    public let totalCost: Double
    public let totalRequests: UInt64

    enum CodingKeys: String, CodingKey {
        case date
        case label
        case models
        case totalInput = "total_input"
        case totalOutput = "total_output"
        case totalThinking = "total_thinking"
        case totalCost = "total_cost"
        case totalRequests = "total_requests"
    }
}

public struct ModelUsage: Codable, Equatable, Sendable {
    public let model: String
    public let rawModel: String
    public let provider: String
    public let inputTokens: UInt64
    public let outputTokens: UInt64
    public let cacheReadTokens: UInt64
    public let cacheCreationTokens: UInt64
    public let thinkingTokens: UInt64
    public let costUSD: Double
    public let requestCount: UInt64

    enum CodingKeys: String, CodingKey {
        case model
        case rawModel = "raw_model"
        case provider
        case inputTokens = "input_tokens"
        case outputTokens = "output_tokens"
        case cacheReadTokens = "cache_read_tokens"
        case cacheCreationTokens = "cache_creation_tokens"
        case thinkingTokens = "thinking_tokens"
        case costUSD = "cost_usd"
        case requestCount = "request_count"
    }
}

public enum StatusMetric: String, CaseIterable, Identifiable, Codable, Sendable {
    case tokens
    case cost
    case combined

    public var id: String { rawValue }
}

public enum StatusScopeSelection: String, CaseIterable, Identifiable, Codable, Sendable {
    case today
    case week
    case month
    case allTime

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .today: "Today"
        case .week: "This week"
        case .month: "This month"
        case .allTime: "All time"
        }
    }
}

public enum StatusDecoder {
    public static func decode(_ data: Data) throws -> StatusReport {
        let report = try JSONDecoder().decode(StatusReport.self, from: data)
        guard report.schemaVersion == 1 else {
            throw StatusDecoderError.unsupportedSchema(report.schemaVersion)
        }
        return report
    }
}

public enum StatusDecoderError: Error, Equatable, LocalizedError {
    case unsupportedSchema(UInt32)

    public var errorDescription: String? {
        switch self {
        case let .unsupportedSchema(version):
            "Unsupported status schema version \(version)"
        }
    }
}

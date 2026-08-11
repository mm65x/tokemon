import Foundation

public struct AppPreferences: Equatable, Sendable {
    public static let minimumRefreshInterval: TimeInterval = 5
    public static let maximumRefreshInterval: TimeInterval = 86_400

    public var executablePath: String
    public var metric: StatusMetric
    public var scope: StatusScopeSelection
    public var refreshInterval: TimeInterval

    public init(
        executablePath: String = "",
        metric: StatusMetric = .combined,
        scope: StatusScopeSelection = .today,
        refreshInterval: TimeInterval = 60
    ) {
        self.executablePath = executablePath
        self.metric = metric
        self.scope = scope
        self.refreshInterval = Self.clampedRefreshInterval(refreshInterval)
    }

    public static func load(from defaults: UserDefaults = .standard) -> AppPreferences {
        AppPreferences(
            executablePath: defaults.string(forKey: Keys.executablePath) ?? "",
            metric: StatusMetric(rawValue: defaults.string(forKey: Keys.metric) ?? "") ?? .combined,
            scope: StatusScopeSelection(rawValue: defaults.string(forKey: Keys.scope) ?? "") ?? .today,
            refreshInterval: defaults.double(forKey: Keys.refreshInterval).nonZeroOr(60)
        )
    }

    public func save(to defaults: UserDefaults = .standard) {
        defaults.set(executablePath, forKey: Keys.executablePath)
        defaults.set(metric.rawValue, forKey: Keys.metric)
        defaults.set(scope.rawValue, forKey: Keys.scope)
        defaults.set(refreshInterval, forKey: Keys.refreshInterval)
    }

    private static func clampedRefreshInterval(_ interval: TimeInterval) -> TimeInterval {
        min(max(interval, minimumRefreshInterval), maximumRefreshInterval)
    }

    private enum Keys {
        static let executablePath = "executablePath"
        static let metric = "metric"
        static let scope = "scope"
        static let refreshInterval = "refreshInterval"
    }
}

private extension TimeInterval {
    func nonZeroOr(_ fallback: TimeInterval) -> TimeInterval {
        self > 0 ? self : fallback
    }
}

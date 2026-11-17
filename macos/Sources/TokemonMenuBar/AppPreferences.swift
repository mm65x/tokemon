import Foundation

public struct AppPreferences: Equatable, Sendable {
    public static let minimumRefreshInterval: TimeInterval = 5
    public static let maximumRefreshInterval: TimeInterval = 86_400

    public var executablePath: String
    public var metric: StatusMetric
    public var scope: StatusScopeSelection
    public var refreshInterval: TimeInterval
    public var providers: [String]

    public init(
        executablePath: String = "",
        metric: StatusMetric = .combined,
        scope: StatusScopeSelection = .today,
        refreshInterval: TimeInterval = 60,
        providers: [String] = []
    ) {
        self.executablePath = executablePath
        self.metric = metric
        self.scope = scope
        self.refreshInterval = Self.clampedRefreshInterval(refreshInterval)
        self.providers = Self.normalizedProviders(providers)
    }

    public static func load(from defaults: UserDefaults = .standard) -> AppPreferences {
        AppPreferences(
            executablePath: defaults.string(forKey: Keys.executablePath) ?? "",
            metric: StatusMetric(rawValue: defaults.string(forKey: Keys.metric) ?? "") ?? .combined,
            scope: StatusScopeSelection(rawValue: defaults.string(forKey: Keys.scope) ?? "") ?? .today,
            refreshInterval: defaults.double(forKey: Keys.refreshInterval).nonZeroOr(60),
            providers: defaults.stringArray(forKey: Keys.providers) ?? []
        )
    }

    public func save(to defaults: UserDefaults = .standard) {
        defaults.set(executablePath, forKey: Keys.executablePath)
        defaults.set(metric.rawValue, forKey: Keys.metric)
        defaults.set(scope.rawValue, forKey: Keys.scope)
        defaults.set(refreshInterval, forKey: Keys.refreshInterval)
        defaults.set(Self.normalizedProviders(providers), forKey: Keys.providers)
    }

    public static func normalizedProviders(_ providers: [String]) -> [String] {
        var seen = Set<String>()
        return providers.compactMap { provider in
            let trimmed = provider.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty, seen.insert(trimmed.lowercased()).inserted else { return nil }
            return trimmed
        }
    }

    private static func clampedRefreshInterval(_ interval: TimeInterval) -> TimeInterval {
        min(max(interval, minimumRefreshInterval), maximumRefreshInterval)
    }

    private enum Keys {
        static let executablePath = "tokemonMenuBar.executablePath"
        static let metric = "tokemonMenuBar.metric"
        static let scope = "tokemonMenuBar.scope"
        static let refreshInterval = "tokemonMenuBar.refreshInterval"
        static let providers = "tokemonMenuBar.providers"
    }
}

private extension TimeInterval {
    func nonZeroOr(_ fallback: TimeInterval) -> TimeInterval {
        self > 0 ? self : fallback
    }
}

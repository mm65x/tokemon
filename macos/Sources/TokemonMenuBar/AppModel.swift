import Combine
import Foundation

public enum StatusLoadState: Equatable, Sendable {
    case idle
    case loading
    case loaded
    case failed(String)
}

private enum RefreshOutcome: Sendable {
    case success(StatusCommandResult)
    case failure(String)
}

@MainActor
public final class AppModel: ObservableObject {
    @Published public private(set) var report: StatusReport?
    @Published public private(set) var state: StatusLoadState = .idle
    @Published public private(set) var lastRefresh: Date?
    @Published public var preferences: AppPreferences
    @Published public var showingPreferences = false

    private let defaults: UserDefaults
    private let runnerOverride: (any StatusRunning)?
    private let clock: @Sendable () -> Date
    private var refreshTask: Task<Void, Never>?
    private var timerTask: Task<Void, Never>?

    public init(
        preferences: AppPreferences = .load(),
        defaults: UserDefaults = .standard,
        runner: (any StatusRunning)? = nil,
        clock: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.preferences = preferences
        self.defaults = defaults
        self.runnerOverride = runner
        self.clock = clock
    }

    deinit {
        refreshTask?.cancel()
        timerTask?.cancel()
    }

    public func start() {
        guard timerTask == nil else { return }

        refresh()
        timerTask = Task { [weak self] in
            while !Task.isCancelled {
                let interval = self?.preferences.refreshInterval ?? AppPreferences.minimumRefreshInterval
                let nanoseconds = UInt64(max(interval, AppPreferences.minimumRefreshInterval) * 1_000_000_000)
                do {
                    try await Task.sleep(nanoseconds: nanoseconds)
                } catch {
                    return
                }
                guard !Task.isCancelled else { return }
                self?.refresh()
            }
        }
    }

    public func stop() {
        timerTask?.cancel()
        timerTask = nil
        refreshTask?.cancel()
        refreshTask = nil
    }

    public func refresh() {
        guard refreshTask == nil else { return }

        state = .loading
        let preferences = preferences
        let runnerOverride = runnerOverride
        let now = clock()
        refreshTask = Task { [weak self] in
            let outcome = await Task.detached(priority: .utility) {
                Self.loadStatus(
                    preferences: preferences,
                    runnerOverride: runnerOverride,
                    now: now
                )
            }.value

            guard !Task.isCancelled else { return }
            self?.apply(outcome, refreshedAt: now)
            self?.refreshTask = nil
        }
    }

    public func apply(_ preferences: AppPreferences) {
        self.preferences = preferences
        preferences.save(to: defaults)
        restartTimer()
        refresh()
    }

    public var menuBarTitle: String {
        if report == nil, state == .loading {
            return "Loading…"
        }
        guard report != nil else { return "Tokemon" }
        return metricValue
    }

    public var metricValue: String {
        guard let report else { return "No usage data" }

        switch preferences.metric {
        case .tokens:
            return Self.formatTokens(report.totalTokens)
        case .cost:
            return report.capabilities.cost ? Self.formatCost(report.totalCost) : "Cost unavailable"
        case .combined:
            let tokens = Self.formatTokens(report.totalTokens)
            let cost = report.capabilities.cost ? Self.formatCost(report.totalCost) : "Cost unavailable"
            return "\(tokens) · \(cost)"
        }
    }

    public var isEmpty: Bool {
        report?.state == "empty"
    }

    public var statusMessage: String? {
        if case let .failed(message) = state { return message }
        return nil
    }

    public var updatedLabel: String {
        guard let lastRefresh else { return "Not refreshed yet" }
        return Self.formatDate(lastRefresh)
    }

    public var providerLabel: String {
        guard let report else { return "No providers" }
        let count = report.providers.count
        return "\(count) provider\(count == 1 ? "" : "s")"
    }

    public func resolvedExecutableURL() -> URL? {
        Self.resolveExecutable(path: preferences.executablePath)
    }

    public func dashboardExecutableURL() -> URL? {
        resolvedExecutableURL()
    }

    nonisolated public static func resolveExecutable(
        path: String,
        fileManager: FileManager = .default,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> URL? {
        let expandedPath = path.replacingOccurrences(of: "~", with: NSHomeDirectory(), options: [.anchored])
        if expandedPath.contains("/") {
            if let url = isExecutable(expandedPath, fileManager: fileManager) {
                return url
            }
        }

        let executableName: String
        if expandedPath.isEmpty {
            executableName = "tokemon"
        } else if expandedPath.contains("/") {
            executableName = URL(fileURLWithPath: expandedPath).lastPathComponent
        } else {
            executableName = expandedPath
        }
        let searchPath = environment["PATH"] ?? "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin"
        for directory in searchPath.split(separator: ":") {
            let candidate = URL(fileURLWithPath: String(directory)).appendingPathComponent(executableName)
            if fileManager.isExecutableFile(atPath: candidate.path) {
                return candidate
            }
        }
        return nil
    }

    private func restartTimer() {
        guard timerTask != nil else { return }
        timerTask?.cancel()
        timerTask = nil
        start()
    }

    private func apply(_ outcome: RefreshOutcome, refreshedAt: Date) {
        switch outcome {
        case let .success(result):
            report = result.report
            state = .loaded
            lastRefresh = refreshedAt
        case let .failure(message):
            state = .failed(message)
        }
    }

    nonisolated private static func loadStatus(
        preferences: AppPreferences,
        runnerOverride: (any StatusRunning)?,
        now: Date
    ) -> RefreshOutcome {
        do {
            let runner: any StatusRunning
            if let runnerOverride {
                runner = runnerOverride
            } else if let executableURL = resolveExecutable(path: preferences.executablePath) {
                runner = StatusRunner(executableURL: executableURL)
            } else {
                return .failure(executableError(path: preferences.executablePath))
            }

            let result = try runner.runOffline(
                arguments: StatusQuery(scope: preferences.scope, providers: preferences.providers).arguments(now: now)
            )
            return .success(result)
        } catch {
            return .failure(error.localizedDescription)
        }
    }

    nonisolated private static func isExecutable(_ path: String, fileManager: FileManager) -> URL? {
        guard !path.isEmpty, fileManager.isExecutableFile(atPath: path) else { return nil }
        return URL(fileURLWithPath: path)
    }

    nonisolated private static func executableError(path: String) -> String {
        let expandedPath = path.replacingOccurrences(of: "~", with: NSHomeDirectory(), options: [.anchored])
        guard !expandedPath.isEmpty else {
            return "Could not find tokemon on PATH. Set its executable path in Preferences."
        }
        let fileManager = FileManager.default
        if !fileManager.fileExists(atPath: expandedPath) {
            return "The configured tokemon executable was not found. Update its path in Preferences."
        }
        return "The configured tokemon path is not executable. Update its path in Preferences."
    }

    private static func formatTokens(_ tokens: UInt64) -> String {
        switch tokens {
        case 1_000_000_000...:
            String(format: "%.1fB tok", Double(tokens) / 1_000_000_000)
        case 1_000_000...:
            String(format: "%.1fM tok", Double(tokens) / 1_000_000)
        case 1_000...:
            String(format: "%.1fK tok", Double(tokens) / 1_000)
        default:
            "\(tokens) tok"
        }
    }

    private static func formatCost(_ cost: Double) -> String {
        String(format: "$%.2f", cost)
    }

    private static func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .short
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }
}

import Foundation

public struct StatusQuery: Equatable, Sendable {
    public let scope: StatusScopeSelection
    public let providers: [String]

    public init(scope: StatusScopeSelection, providers: [String] = []) {
        self.scope = scope
        self.providers = AppPreferences.normalizedProviders(providers)
    }

    public func arguments(now: Date = Date()) -> [String] {
        let calendar = Self.utcCalendar
        let today = calendar.startOfDay(for: now)
        let todayString = Self.dateString(today)

        let scopeArguments: [String]
        switch scope {
        case .today:
            scopeArguments = ["--frequency", "daily", "--since", todayString, "--until", todayString]
        case .week:
            let weekday = calendar.component(.weekday, from: today)
            let daysFromMonday = (weekday + 5) % 7
            let start = calendar.date(byAdding: .day, value: -daysFromMonday, to: today) ?? today
            scopeArguments = [
                "--frequency", "weekly",
                "--since", Self.dateString(start),
                "--until", todayString
            ]
        case .month:
            let components = calendar.dateComponents([.year, .month], from: today)
            let start = calendar.date(from: components) ?? today
            scopeArguments = [
                "--frequency", "monthly",
                "--since", Self.dateString(start),
                "--until", todayString
            ]
        case .allTime:
            scopeArguments = ["--frequency", "monthly"]
        }

        return scopeArguments + providers.flatMap { ["--provider", $0] }
    }

    private static let utcCalendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? .gmt
        calendar.locale = Locale(identifier: "en_US_POSIX")
        return calendar
    }()

    private static func dateString(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.calendar = utcCalendar
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = utcCalendar.timeZone
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }
}

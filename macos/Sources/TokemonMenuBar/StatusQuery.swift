import Foundation

public struct StatusQuery: Equatable, Sendable {
    public let scope: StatusScopeSelection

    public init(scope: StatusScopeSelection) {
        self.scope = scope
    }

    public func arguments(now: Date = Date()) -> [String] {
        let calendar = Self.utcCalendar
        let today = calendar.startOfDay(for: now)
        let todayString = Self.dateString(today)

        switch scope {
        case .today:
            return ["--frequency", "daily", "--since", todayString, "--until", todayString]
        case .week:
            let weekday = calendar.component(.weekday, from: today)
            let daysFromMonday = (weekday + 5) % 7
            let start = calendar.date(byAdding: .day, value: -daysFromMonday, to: today) ?? today
            return [
                "--frequency", "weekly",
                "--since", Self.dateString(start),
                "--until", todayString
            ]
        case .month:
            let components = calendar.dateComponents([.year, .month], from: today)
            let start = calendar.date(from: components) ?? today
            return [
                "--frequency", "monthly",
                "--since", Self.dateString(start),
                "--until", todayString
            ]
        case .allTime:
            return ["--frequency", "monthly"]
        }
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

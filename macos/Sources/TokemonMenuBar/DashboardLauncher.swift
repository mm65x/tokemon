import Foundation

public enum DashboardLauncherError: Error, LocalizedError, Equatable {
    case invalidExecutable
    case launchFailed(String)

    public var errorDescription: String? {
        switch self {
        case .invalidExecutable:
            "Set the tokemon executable path in Preferences before opening the dashboard."
        case let .launchFailed(message):
            "Could not open the dashboard: \(message)"
        }
    }
}

public enum DashboardLauncher {
    public static func launch(executableURL: URL) throws {
        guard !executableURL.path.isEmpty else {
            throw DashboardLauncherError.invalidExecutable
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        process.arguments = commandArguments(for: executableURL)
        do {
            try process.run()
        } catch {
            throw DashboardLauncherError.launchFailed(error.localizedDescription)
        }
    }

    public static func commandArguments(for executableURL: URL) -> [String] {
        ["-a", "Terminal", executableURL.path, "--args", "top"]
    }
}

import Foundation

public struct StatusCommandResult: Equatable, Sendable {
    public let report: StatusReport
    public let stderr: String
}

public enum StatusRunnerError: Error, Equatable, LocalizedError {
    case invalidExecutablePath
    case launchFailed(String)
    case timedOut
    case outputTooLarge
    case nonZeroExit(Int32, String)
    case invalidOutputEncoding

    public var errorDescription: String? {
        switch self {
        case .invalidExecutablePath:
            "The configured executable path is invalid"
        case let .launchFailed(message):
            "Could not start tokemon: \(message)"
        case .timedOut:
            "The status command timed out"
        case .outputTooLarge:
            "The status response exceeded the output limit"
        case let .nonZeroExit(code, message):
            message.isEmpty ? "The status command exited with code \(code)" : message
        case .invalidOutputEncoding:
            "The status command returned non-UTF-8 output"
        }
    }
}

private final class DataCollector: @unchecked Sendable {
    private let lock = NSLock()
    private var value = Data()

    func append(_ data: Data) {
        lock.lock()
        value.append(data)
        lock.unlock()
    }

    func data() -> Data {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}

public final class StatusRunner {
    public let executableURL: URL
    public let timeout: TimeInterval
    public let outputLimit: Int

    public init(
        executableURL: URL,
        timeout: TimeInterval = 5,
        outputLimit: Int = 1_048_576
    ) {
        self.executableURL = executableURL
        self.timeout = timeout
        self.outputLimit = outputLimit
    }

    public func run(arguments: [String]) throws -> StatusCommandResult {
        guard !executableURL.path.isEmpty else {
            throw StatusRunnerError.invalidExecutablePath
        }

        let process = Process()
        process.executableURL = executableURL
        process.arguments = arguments

        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr

        let stdoutCollector = DataCollector()
        let stderrCollector = DataCollector()
        let readGroup = DispatchGroup()

        readGroup.enter()
        DispatchQueue.global(qos: .utility).async {
            stdoutCollector.append(stdout.fileHandleForReading.readDataToEndOfFile())
            readGroup.leave()
        }
        readGroup.enter()
        DispatchQueue.global(qos: .utility).async {
            stderrCollector.append(stderr.fileHandleForReading.readDataToEndOfFile())
            readGroup.leave()
        }

        var timedOut = false
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue.global(qos: .utility))
        timer.schedule(deadline: .now() + timeout)
        timer.setEventHandler {
            if process.isRunning {
                timedOut = true
                process.terminate()
            }
        }
        timer.resume()

        do {
            try process.run()
        } catch {
            timer.cancel()
            throw StatusRunnerError.launchFailed(error.localizedDescription)
        }

        process.waitUntilExit()
        timer.cancel()
        readGroup.wait()

        let output = stdoutCollector.data()
        let errorOutput = stderrCollector.data()
        guard output.count <= outputLimit else {
            throw StatusRunnerError.outputTooLarge
        }
        if timedOut {
            throw StatusRunnerError.timedOut
        }

        let stderrText = String(data: errorOutput, encoding: .utf8) ?? ""
        guard process.terminationStatus == 0 else {
            throw StatusRunnerError.nonZeroExit(process.terminationStatus, stderrText)
        }
        guard String(data: output, encoding: .utf8) != nil else {
            throw StatusRunnerError.invalidOutputEncoding
        }

        do {
            return StatusCommandResult(
                report: try StatusDecoder.decode(output),
                stderr: stderrText
            )
        } catch {
            throw error
        }
    }

    public func runOffline(arguments: [String] = []) throws -> StatusCommandResult {
        try run(arguments: ["status", "--json", "--offline"] + arguments)
    }
}

import AppKit
import SwiftUI

struct MenuContentView: View {
    @ObservedObject var model: AppModel
    @State private var dashboardError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.metricValue)
                        .font(.headline)
                        .monospacedDigit()
                    Text("\(model.preferences.scope.label) · \(model.updatedLabel)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if model.state == .loading {
                    ProgressView()
                        .controlSize(.small)
                }
            }

            if model.isEmpty {
                Label("No usage data found", systemImage: "chart.bar.xaxis")
                    .foregroundStyle(.secondary)
            }

            if let message = model.statusMessage {
                Label(message, systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Divider()

            Picker("Metric", selection: metricBinding) {
                Text("Tokens").tag(StatusMetric.tokens)
                Text("Cost").tag(StatusMetric.cost)
                Text("Combined").tag(StatusMetric.combined)
            }

            Picker("Scope", selection: scopeBinding) {
                ForEach(StatusScopeSelection.allCases) { scope in
                    Text(scope.label).tag(scope)
                }
            }

            Button("Refresh") {
                model.refresh()
            }
            .keyboardShortcut("r")

            Button("Open dashboard") {
                dashboardError = launchDashboard()
            }

            Button("Preferences…") {
                model.showingPreferences = true
            }

            if let dashboardError {
                Text(dashboardError)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Divider()

            Button("Quit Tokemon") {
                NSApplication.shared.terminate(nil)
            }
        }
        .padding(16)
        .frame(minWidth: 280)
        .sheet(isPresented: $model.showingPreferences) {
            PreferencesView(model: model)
        }
    }

    private var metricBinding: Binding<StatusMetric> {
        Binding(
            get: { model.preferences.metric },
            set: { value in
                var preferences = model.preferences
                preferences.metric = value
                model.apply(preferences)
            }
        )
    }

    private var scopeBinding: Binding<StatusScopeSelection> {
        Binding(
            get: { model.preferences.scope },
            set: { value in
                var preferences = model.preferences
                preferences.scope = value
                model.apply(preferences)
            }
        )
    }

    private func launchDashboard() -> String? {
        guard let executableURL = model.dashboardExecutableURL() else {
            return DashboardLauncherError.invalidExecutable.localizedDescription
        }
        do {
            try DashboardLauncher.launch(executableURL: executableURL)
            return nil
        } catch {
            return error.localizedDescription
        }
    }
}

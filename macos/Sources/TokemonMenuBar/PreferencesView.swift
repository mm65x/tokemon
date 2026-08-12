import SwiftUI

struct PreferencesView: View {
    @ObservedObject var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var draft: AppPreferences

    init(model: AppModel) {
        self.model = model
        _draft = State(initialValue: model.preferences)
    }

    var body: some View {
        Form {
            Section("Status executable") {
                TextField("Path", text: $draft.executablePath)
                    .textFieldStyle(.roundedBorder)
                HStack {
                    Button("Detect") {
                        if let url = AppModel.resolveExecutable(path: "") {
                            draft.executablePath = url.path
                        }
                    }
                    Text("Leave blank to search PATH for tokemon.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Section("Display") {
                Picker("Metric", selection: $draft.metric) {
                    Text("Tokens").tag(StatusMetric.tokens)
                    Text("Cost").tag(StatusMetric.cost)
                    Text("Combined").tag(StatusMetric.combined)
                }
                Picker("Scope", selection: $draft.scope) {
                    ForEach(StatusScopeSelection.allCases) { scope in
                        Text(scope.label).tag(scope)
                    }
                }
            }

            Section("Refresh") {
                HStack {
                    Slider(
                        value: $draft.refreshInterval,
                        in: AppPreferences.minimumRefreshInterval...600,
                        step: 5
                    )
                    Text("\(Int(draft.refreshInterval)) sec")
                        .monospacedDigit()
                        .frame(width: 72, alignment: .trailing)
                }
                Text("Refresh runs locally and uses the offline status command.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            HStack {
                Spacer()
                Button("Cancel") {
                    dismiss()
                }
                Button("Save") {
                    model.apply(draft)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .formStyle(.grouped)
        .padding()
        .frame(width: 480)
    }
}

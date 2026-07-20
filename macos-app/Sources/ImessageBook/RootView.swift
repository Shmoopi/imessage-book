import SwiftUI

struct RootView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            StepBar(current: model.step)
            Divider().opacity(0.5)
            Group {
                switch model.step {
                case .source: SourceStepView()
                case .conversation: ConversationStepView()
                case .options: OptionsStepView()
                case .run: RunStepView()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

/// The "Source › Conversation › Options › Export" progress strip along the top.
private struct StepBar: View {
    let current: WizardStep

    var body: some View {
        HStack(spacing: 10) {
            ForEach(WizardStep.allCases) { step in
                let state = state(for: step)
                HStack(spacing: 7) {
                    ZStack {
                        Circle()
                            .fill(state == .todo ? Color.secondary.opacity(0.12) : Color.accentColor.opacity(0.15))
                            .frame(width: 22, height: 22)
                        if state == .done {
                            Image(systemName: "checkmark").font(.system(size: 11, weight: .bold))
                                .foregroundStyle(Color.accentColor)
                        } else {
                            Text("\(step.rawValue + 1)").font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(state == .todo ? Color.secondary : Color.accentColor)
                        }
                    }
                    Text(step.title)
                        .font(.subheadline)
                        .fontWeight(state == .current ? .semibold : .regular)
                        .foregroundStyle(state == .todo ? Color.secondary : .primary)
                }
                if step != WizardStep.allCases.last {
                    Rectangle().fill(Color.secondary.opacity(0.15))
                        .frame(height: 1).frame(maxWidth: 36)
                }
            }
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity)
    }

    private enum CellState { case done, current, todo }

    private func state(for step: WizardStep) -> CellState {
        if step.rawValue < current.rawValue { return .done }
        if step == current { return .current }
        return .todo
    }
}

import SwiftUI

/// True while rendering documentation screenshots — swaps `ScrollView`/`List` (which
/// `ImageRenderer` can't rasterize offscreen) for plain stacks. No effect in the real app.
private struct ScreenshotModeKey: EnvironmentKey { static let defaultValue = false }
extension EnvironmentValues {
    var screenshotMode: Bool {
        get { self[ScreenshotModeKey.self] }
        set { self[ScreenshotModeKey.self] = newValue }
    }
}

/// Consistent step chrome: a roomy, scrollable titled body over a fixed footer button row.
struct StepLayout<Content: View, Footer: View>: View {
    let title: String
    var subtitle: String?
    @ViewBuilder var content: () -> Content
    @ViewBuilder var footer: () -> Footer

    @Environment(\.screenshotMode) private var screenshotMode

    private var titledBody: some View {
        VStack(alignment: .leading, spacing: 22) {
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.title.weight(.semibold))
                if let subtitle {
                    Text(subtitle)
                        .font(.body)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            content()
        }
        .padding(.horizontal, 30)
        .padding(.vertical, 26)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    var body: some View {
        VStack(spacing: 0) {
            if screenshotMode {
                titledBody.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            } else {
                ScrollView { titledBody }
            }
            Divider().opacity(0.5)
            HStack(spacing: 12) { footer() }
                .padding(.horizontal, 24)
                .padding(.vertical, 16)
        }
    }
}

/// A small uppercase group label above a cluster of controls.
struct SectionLabel: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        Text(text.uppercased())
            .font(.caption.weight(.semibold))
            .foregroundStyle(.tertiary)
            .tracking(0.6)
    }
}

/// A little pill, e.g. "needs Tectonic".
struct Badge: View {
    let text: String
    var tint: Color = .orange
    var body: some View {
        Text(text)
            .font(.caption2.weight(.medium))
            .padding(.horizontal, 7).padding(.vertical, 2)
            .background(tint.opacity(0.15), in: Capsule())
            .foregroundStyle(tint)
    }
}

/// A tappable option card — soft fill, no hard border, checkmark when selected, and a
/// gentle hover highlight. The lightweight building block for source and format choices.
struct SelectableCard: View {
    let symbol: String
    let title: String
    var badge: String?
    let subtitle: String
    let selected: Bool
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Image(systemName: symbol)
                    .font(.title2)
                    .symbolVariant(selected ? .fill : .none)
                    .foregroundStyle(selected ? Color.accentColor : .secondary)
                    .frame(width: 30)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 7) {
                        Text(title).fontWeight(.medium)
                        if let badge { Badge(text: badge) }
                    }
                    Text(subtitle)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 8)
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .font(.title3)
                    .foregroundStyle(selected ? Color.accentColor : Color.secondary.opacity(0.35))
            }
            .padding(15)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 13, style: .continuous)
                    .fill(selected
                          ? Color.accentColor.opacity(0.12)
                          : Color.primary.opacity(hovering ? 0.06 : 0.035))
            )
            .contentShape(RoundedRectangle(cornerRadius: 13, style: .continuous))
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .animation(.easeOut(duration: 0.12), value: selected)
    }
}

// MARK: - Screenshot facsimiles
//
// `ImageRenderer` can't rasterize AppKit-backed controls (TextField, Picker, DatePicker,
// Toggle) — they come out blank. These pure-SwiftUI stand-ins are shown only while
// rendering documentation screenshots (see `screenshotMode`), never in the real app.

struct FauxTextField: View {
    var text: String = ""
    var prompt: String = ""
    var body: some View {
        HStack {
            Text(text.isEmpty ? prompt : text)
                .foregroundStyle(text.isEmpty ? Color.secondary : .primary)
            Spacer()
        }
        .padding(.horizontal, 8).padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color(nsColor: .textBackgroundColor)))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.primary.opacity(0.15)))
    }
}

struct FauxSearchField: View {
    let placeholder: String
    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
            Text(placeholder).foregroundStyle(.secondary)
            Spacer()
        }
        .padding(.horizontal, 8).padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color(nsColor: .textBackgroundColor)))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.primary.opacity(0.15)))
    }
}

struct FauxCheckbox: View {
    let label: String
    var on: Bool = false
    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: on ? "checkmark.square.fill" : "square")
                .foregroundStyle(on ? Color.accentColor : .secondary)
            Text(label)
            Spacer(minLength: 0)
        }
    }
}

struct FauxSegmented: View {
    let options: [String]
    var selected: Int = 0
    var body: some View {
        HStack(spacing: 4) {
            ForEach(Array(options.enumerated()), id: \.offset) { index, option in
                Text(option)
                    .font(.callout)
                    .fontWeight(index == selected ? .medium : .regular)
                    .foregroundStyle(index == selected ? Color.primary : .secondary)
                    .padding(.vertical, 6)
                    .frame(maxWidth: .infinity)
                    .background(index == selected ? Color(nsColor: .controlBackgroundColor) : .clear,
                                in: RoundedRectangle(cornerRadius: 6))
            }
        }
        .padding(3)
        .background(Color.primary.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
        .frame(maxWidth: 360)
    }
}

// MARK: - Banner

enum BannerKind {
    case info, warning, error, success

    var symbol: String {
        switch self {
        case .info: return "info.circle.fill"
        case .warning: return "exclamationmark.triangle.fill"
        case .error: return "xmark.octagon.fill"
        case .success: return "checkmark.circle.fill"
        }
    }

    var tint: Color {
        switch self {
        case .info: return .accentColor
        case .warning: return .orange
        case .error: return .red
        case .success: return .green
        }
    }
}

/// A soft, borderless callout for status and guidance.
struct Banner<Trailing: View>: View {
    let kind: BannerKind
    let title: String
    var message: String?
    @ViewBuilder var trailing: () -> Trailing

    init(_ kind: BannerKind, title: String, message: String? = nil,
         @ViewBuilder trailing: @escaping () -> Trailing) {
        self.kind = kind
        self.title = title
        self.message = message
        self.trailing = trailing
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: kind.symbol)
                .foregroundStyle(kind.tint)
                .font(.title3)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).fontWeight(.medium)
                if let message {
                    Text(message).font(.callout).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 0)
            trailing()
        }
        .padding(15)
        .background(kind.tint.opacity(0.10), in: RoundedRectangle(cornerRadius: 13, style: .continuous))
    }
}

extension Banner where Trailing == EmptyView {
    init(_ kind: BannerKind, title: String, message: String? = nil) {
        self.init(kind, title: title, message: message) { EmptyView() }
    }
}

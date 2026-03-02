import SwiftUI
import Observation
import SpacetimeDB
#if canImport(AppKit)
import AppKit
#endif


struct GameEventEntry: Identifiable {
    enum Kind {
        case info
        case combat
    }

    let id: Int
    let text: String
    let kind: Kind
    let timestamp: Date
}

struct HudStatChip: View {
    let label: String
    let value: String
    let tint: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(label)
                .font(.system(size: 9, weight: .semibold, design: .rounded))
                .foregroundStyle(tint.opacity(0.72))
            Text(value)
                .font(.system(size: 13, weight: .semibold, design: .rounded))
                .foregroundStyle(tint)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 5)
        .background(tint.opacity(0.10))
        .overlay(Rectangle().strokeBorder(tint.opacity(0.42), lineWidth: 2))
    }
}

struct HudHealthMeter: View {
    let health: UInt32

    private var clampedHealth: Double {
        min(100, max(0, Double(health)))
    }

    private var healthFraction: Double {
        clampedHealth / 100
    }

    private var healthColor: Color {
        Color(hue: 0.33 * healthFraction, saturation: 0.82, brightness: 0.95)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text("HP")
                    .font(.system(size: 9, weight: .heavy, design: .rounded))
                    .foregroundStyle(healthColor.opacity(0.80))
                Text("\(Int(clampedHealth))/100")
                    .font(.system(size: 11, weight: .heavy, design: .rounded))
                    .foregroundStyle(healthColor)
            }

            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Rectangle()
                        .fill(Color.black.opacity(0.35))
                    Rectangle()
                        .fill(healthColor)
                        .frame(width: max(4, geo.size.width * healthFraction))
                }
            }
            .frame(height: 7)
            .overlay(Rectangle().strokeBorder(healthColor.opacity(0.50), lineWidth: 1))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(Color.white.opacity(0.06))
        .overlay(Rectangle().strokeBorder(Color(white: 0.28), lineWidth: 1))
    }
}

struct EventFeedView: View {
    let events: [GameEventEntry]
    var title: String = "Event Feed"
    var maxVisible: Int = 8
    var padded: Bool = false
    private let eventLifetime: TimeInterval = 18
    private let fadeDuration: TimeInterval = 10
    private let popInDuration: TimeInterval = 0.35

    struct RenderedEvent: Identifiable {
        let entry: GameEventEntry
        let opacity: Double
        let scale: CGFloat
        let offsetY: CGFloat
        var id: Int { entry.id }
    }

    private func renderedEvents(at now: Date) -> [RenderedEvent] {
        Array(events.suffix(maxVisible).reversed()).compactMap { event in
            let age = now.timeIntervalSince(event.timestamp)
            guard age >= 0, age < eventLifetime else { return nil }

            let fadeStart = eventLifetime - fadeDuration
            let opacity: Double
            if age <= fadeStart {
                opacity = 1.0
            } else {
                opacity = max(0, (eventLifetime - age) / max(0.001, fadeDuration))
            }

            let popProgress = min(1, max(0, age / popInDuration))
            let popEase = 1 - pow(1 - popProgress, 3)
            let scale = 0.94 + (0.06 * popEase)
            let offsetY = 8 * (1 - popEase)

            return RenderedEvent(
                entry: event,
                opacity: opacity,
                scale: scale,
                offsetY: offsetY
            )
        }
    }

    private var listHeight: CGFloat {
        // Stable height prevents panel-edge jitter as items appear/disappear.
        CGFloat(maxVisible) * 18 + 4
    }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.25)) { timeline in
            let visible = renderedEvents(at: timeline.date)
            VStack(alignment: .leading, spacing: 6) {
                Text(title)
                    .font(.system(size: 10, weight: .semibold, design: .rounded))
                    .foregroundStyle(Color(white: 0.72))
                    .shadow(color: .black, radius: 2, x: 1, y: 1)

                ZStack(alignment: .topLeading) {
                    if visible.isEmpty {
                        Text("No recent events")
                            .font(.system(size: 10, weight: .medium, design: .rounded))
                            .foregroundStyle(Color(white: 0.35))
                            .shadow(color: .black.opacity(0.8), radius: 1, x: 1, y: 1)
                    }

                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(visible) { item in
                            HStack(spacing: 6) {
                                Text(item.entry.kind == .combat ? "►" : "·")
                                    .font(.system(size: 9, weight: .heavy, design: .rounded))
                                    .foregroundStyle(item.entry.kind == .combat ? Color.orange : SurvivorsTheme.accent)
                                    .shadow(color: .black.opacity(0.8), radius: 1, x: 0, y: 1)
                                Text(item.entry.text)
                                    .font(.system(size: 10, weight: .medium, design: .rounded))
                                    .foregroundStyle(.white)
                                    .lineLimit(1)
                                    .shadow(color: .black, radius: 1.5, x: 1, y: 1)
                                Spacer(minLength: 0)
                            }
                            .opacity(item.opacity)
                            .scaleEffect(item.scale, anchor: .leading)
                            .offset(y: item.offsetY)
                        }
                    }
                }
                .frame(height: listHeight, alignment: .topLeading)
            }
            .animation(.spring(response: 0.28, dampingFraction: 0.86), value: visible.map(\.id))
            .padding(padded ? 10 : 0)
        }
    }
}

struct MenuButton: View {
    let title: String
    let systemImage: String
    var role: ButtonRole? = nil
    let action: () -> Void

    var body: some View {
        Button(role: role, action: action) {
            HStack(spacing: 8) {
                Image(systemName: systemImage)
                Text(title)
                Spacer()
            }
        }
        .buttonStyle(PixelButtonStyle(danger: role == .some(.destructive)))
        .controlSize(.large)
        .frame(maxWidth: .infinity)
    }
}

extension Color {
    static func fromId(_ id: UInt64) -> Color {
        let h = Double(id % 360) / 360.0
        return Color(hue: h, saturation: 0.72, brightness: 0.88)
    }
}

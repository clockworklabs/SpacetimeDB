import SwiftUI
import Observation

// MARK: - Effect Manager

@MainActor
@Observable
class EffectManager {
    static let shared = EffectManager()
    
    struct ActiveEffect: Identifiable {
        enum Kind {
            case particle(color: Color, velocity: CGVector)
            case floatingText(text: String, color: Color)
        }
        let id = UUID()
        let kind: Kind
        var x: CGFloat
        var y: CGFloat
        var opacity: Double = 1.0
        var scale: CGFloat = 1.0
        var createdAt: TimeInterval
        var lifetime: TimeInterval
    }
    
    private(set) var activeEffects: [ActiveEffect] = []
    
    func spawnHit(x: Float, y: Float, value: String?) {
        let now = Date.timeIntervalSinceReferenceDate
        // Floating text
        activeEffects.append(ActiveEffect(
            kind: .floatingText(text: value ?? "HIT", color: .red),
            x: CGFloat(x),
            y: CGFloat(y) - 20,
            createdAt: now,
            lifetime: 0.8
        ))
        
        // Red particles
        for _ in 0..<6 {
            activeEffects.append(ActiveEffect(
                kind: .particle(color: .red, velocity: CGVector(dx: CGFloat.random(in: -40...40), dy: CGFloat.random(in: -40...40))),
                x: CGFloat(x),
                y: CGFloat(y),
                createdAt: now,
                lifetime: 0.4
            ))
        }
    }
    
    func spawnKill(x: Float, y: Float) {
        let now = Date.timeIntervalSinceReferenceDate
        activeEffects.append(ActiveEffect(
            kind: .floatingText(text: "KILL!", color: .orange),
            x: CGFloat(x),
            y: CGFloat(y) - 30,
            createdAt: now,
            lifetime: 1.2
        ))
        
        // Gold particles
        for _ in 0..<12 {
            activeEffects.append(ActiveEffect(
                kind: .particle(color: .orange, velocity: CGVector(dx: CGFloat.random(in: -60...60), dy: CGFloat.random(in: -60...60))),
                x: CGFloat(x),
                y: CGFloat(y),
                createdAt: now,
                lifetime: 0.6
            ))
        }
    }
    
    func spawnPickup(x: Float, y: Float, value: String) {
        let now = Date.timeIntervalSinceReferenceDate
        activeEffects.append(ActiveEffect(
            kind: .floatingText(text: value, color: Color(red: 0.55, green: 0.82, blue: 1.0)),
            x: CGFloat(x),
            y: CGFloat(y) - 20,
            createdAt: now,
            lifetime: 1.0
        ))
    }
    
    func spawnDeath(x: Float, y: Float) {
        let now = Date.timeIntervalSinceReferenceDate
        activeEffects.append(ActiveEffect(
            kind: .floatingText(text: "ELIMINATED", color: .gray),
            x: CGFloat(x),
            y: CGFloat(y) - 20,
            createdAt: now,
            lifetime: 1.5
        ))
        
        // Dark/smoke particles
        for _ in 0..<20 {
            activeEffects.append(ActiveEffect(
                kind: .particle(color: Color(white: 0.2), velocity: CGVector(dx: CGFloat.random(in: -80...80), dy: CGFloat.random(in: -80...80))),
                x: CGFloat(x),
                y: CGFloat(y),
                createdAt: now,
                lifetime: 1.0
            ))
        }
    }
    
    func update(dt: Double, now: TimeInterval) {
        activeEffects = activeEffects.compactMap { effect in
            let age = now - effect.createdAt
            guard age < effect.lifetime else { return nil }
            
            var updated = effect
            let progress = age / effect.lifetime
            updated.opacity = 1.0 - pow(progress, 2)
            
            switch effect.kind {
            case .particle(_, let velocity):
                updated.x += velocity.dx * CGFloat(dt)
                updated.y += velocity.dy * CGFloat(dt)
                updated.scale = 1.0 - progress
            case .floatingText:
                updated.y -= 30 * CGFloat(dt) // Float up
                updated.scale = 1.0 + (0.2 * progress) // Grow slightly
            }
            
            return updated
        }
    }
}

// MARK: - Effect Views

struct EffectOverlayView: View {
    let effects: [EffectManager.ActiveEffect]
    let camX: CGFloat
    let camY: CGFloat
    let zoom: CGFloat
    let viewportSize: CGSize
    
    var body: some View {
        ZStack {
            ForEach(effects) { effect in
                let screenX = (effect.x - camX) * zoom
                let screenY = (effect.y - camY) * zoom
                let screenMargin: CGFloat = 42
                if screenX >= -screenMargin &&
                    screenX <= viewportSize.width + screenMargin &&
                    screenY >= -screenMargin &&
                    screenY <= viewportSize.height + screenMargin {
                    Group {
                        switch effect.kind {
                        case .particle(let color, _):
                            Rectangle()
                                .fill(color)
                                .frame(width: 4 * zoom * effect.scale, height: 4 * zoom * effect.scale)
                        case .floatingText(let text, let color):
                            Text(text.uppercased())
                                .font(.system(size: 10 * zoom, weight: .heavy, design: .rounded))
                                .foregroundStyle(color)
                                .shadow(color: .black, radius: 2, x: 1, y: 1)
                        }
                    }
                    .opacity(effect.opacity)
                    .position(
                        x: screenX,
                        y: screenY
                    )
                }
            }
        }
        .allowsHitTesting(false)
    }
}

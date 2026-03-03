import SwiftUI
import Observation
import SpacetimeDB
#if canImport(AppKit)
import AppKit
#endif

private let gameplayZoom: CGFloat = 1.9
private let baseEntitySpriteSize: CGFloat = 58

fileprivate struct VisiblePlayerSnapshot: Identifiable {
    let id: UInt64
    let player: Player
    let worldX: Float
    let worldY: Float
    let direction: NinjaGameViewModel.NinjaDirection
    let isMoving: Bool
    let isFlashing: Bool
    let color: Color
}

// MARK: - Sword orbit layout

/// Returns 2D offsets for `count` swords arranged in concentric rings.
///
/// Ring packing: the innermost ring (radius 30) holds as many swords as fit
/// with at least 28pt of arc spacing between them. Each subsequent ring is
/// 25pt further out and holds proportionally more swords. All rings rotate
/// together at ~2 s per revolution.
@inline(__always)
func forEachSwordPosition(count: Int, t: TimeInterval, _ body: (CGPoint) -> Void) {
    guard count > 0 else { return }

    @inline(__always)
    func capacity(_ radius: CGFloat) -> Int {
        max(1, Int((2 * .pi * radius) / 28))
    }

    var rings: [(radius: CGFloat, cap: Int)] = []
    rings.reserveCapacity(4)
    var slots = 0
    var r: CGFloat = 50
    while slots < count {
        let cap = capacity(r)
        rings.append((r, cap))
        slots += cap
        r += 25
    }

    var remaining = count
    let baseAngle = t * .pi

    for ring in rings {
        guard remaining > 0 else { break }
        let inThisRing = min(remaining, ring.cap)
        remaining -= inThisRing
        for i in 0..<inThisRing {
            let angle = baseAngle + (2 * .pi / Double(inThisRing)) * Double(i)
            body(
                CGPoint(
                    x: cos(angle) * ring.radius,
                    y: sin(angle) * ring.radius
                )
            )
        }
    }
}

func swordPositions(count: Int, t: TimeInterval) -> [CGPoint] {
    var positions: [CGPoint] = []
    positions.reserveCapacity(max(0, count))
    forEachSwordPosition(count: count, t: t) { positions.append($0) }
    return positions
}

struct SwiftUIGameViewport: View {
    let vm: NinjaGameViewModel
    @State private var lastFrameTimestamp: TimeInterval?
    @State private var frameMs: Double = 0
    private static let showPerfHUD = ProcessInfo.processInfo.environment["NINJA_PERF_HUD"] == "1"
    private static let renderInterval: TimeInterval = 1.0 / 120.0

    var body: some View {
        GeometryReader { geo in
            TimelineView(.animation(minimumInterval: Self.renderInterval, paused: false)) { timeline in
                let t = timeline.date.timeIntervalSinceReferenceDate
                let showPerfHUD = Self.showPerfHUD
                let worldViewportSize = CGSize(
                    width: geo.size.width / gameplayZoom,
                    height: geo.size.height / gameplayZoom
                )
                let camera = cameraOrigin(viewportSize: worldViewportSize)
                let camX = camera.x
                let camY = camera.y
                let activeEffects = EffectManager.shared.activeEffects
                let visibleWeapons = vm.weapons.filter { weapon in
                    isWorldPointVisible(
                        x: CGFloat(weapon.x),
                        y: CGFloat(weapon.y),
                        camX: camX,
                        camY: camY,
                        viewportWorldSize: worldViewportSize,
                        padding: 42
                    )
                }
                let visiblePlayers = vm.renderPlayers.compactMap { player -> VisiblePlayerSnapshot? in
                    let worldX: Float = {
                        if player.id == vm.userId && vm.hasJoined { return vm.localX }
                        return vm.smoothedPositions[player.id]?.x ?? player.x
                    }()
                    let worldY: Float = {
                        if player.id == vm.userId && vm.hasJoined { return vm.localY }
                        return vm.smoothedPositions[player.id]?.y ?? player.y
                    }()
                    guard isWorldPointVisible(
                        x: CGFloat(worldX),
                        y: CGFloat(worldY),
                        camX: camX,
                        camY: camY,
                        viewportWorldSize: worldViewportSize,
                        padding: 86
                    ) else {
                        return nil
                    }
                    return VisiblePlayerSnapshot(
                        id: player.id,
                        player: player,
                        worldX: worldX,
                        worldY: worldY,
                        direction: vm.playerDirections[player.id] ?? .south,
                        isMoving: vm.playerIsMoving[player.id] ?? false,
                        isFlashing: vm.playerIsHitFlashing(player.id, at: t),
                        color: Color.fromId(player.id)
                    )
                }
                let visibleEffectsCount = showPerfHUD
                    ? countVisibleEffects(
                        effects: activeEffects,
                        camX: camX,
                        camY: camY,
                        zoom: gameplayZoom,
                        viewportSize: geo.size
                    )
                    : 0

                ZStack {
                    ProceduralWorldBackdrop(
                        camX: camX,
                        camY: camY,
                        zoom: gameplayZoom
                    )

                    GameEntitiesCanvas(
                        players: visiblePlayers,
                        weapons: visibleWeapons,
                        t: t,
                        camX: camX,
                        camY: camY,
                        zoom: gameplayZoom
                    )

                    ForEach(visiblePlayers) { snapshot in
                        PlayerLabelsView(player: snapshot.player)
                            .position(
                                x: (CGFloat(snapshot.worldX) - camX) * gameplayZoom,
                                y: (CGFloat(snapshot.worldY) - camY) * gameplayZoom - 46 * gameplayZoom
                            )
                    }

                    EffectOverlayView(
                        effects: activeEffects,
                        camX: camX,
                        camY: camY,
                        zoom: gameplayZoom,
                        viewportSize: geo.size
                    )

                    if showPerfHUD {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(String(format: "frame %.1f ms", frameMs))
                            Text("players vis \(visiblePlayers.count) / total \(vm.players.count)")
                            Text("weapons vis \(visibleWeapons.count) / total \(vm.weapons.count)")
                            Text("effects vis \(visibleEffectsCount) / total \(activeEffects.count)")
                            Text("collision \(vm.isCollisionComputeInFlight ? "busy" : "idle")")
                        }
                        .font(.system(size: 10, weight: .bold, design: .monospaced))
                        .foregroundStyle(Color(red: 0.66, green: 0.96, blue: 0.90))
                        .padding(.horizontal, 8)
                        .padding(.vertical, 6)
                        .background(Color.black.opacity(0.62))
                        .overlay(Rectangle().strokeBorder(Color(red: 0.25, green: 0.80, blue: 0.78).opacity(0.9), lineWidth: 1))
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                        .padding(.top, 72)
                        .padding(.trailing, 10)
                    }
                }
                .onChange(of: t) { _, _ in
                    let dt = max(0, min(0.05, t - (lastFrameTimestamp ?? t)))
                    EffectManager.shared.update(dt: dt, now: t)
                    if let last = lastFrameTimestamp {
                        frameMs = max(0, (t - last) * 1000.0)
                    }
                    lastFrameTimestamp = t
                }
                .frame(
                    width: geo.size.width,
                    height: geo.size.height,
                    alignment: .topLeading
                )
                .clipped()
                .allowsHitTesting(false)
            }
        }
    }

    private func cameraOrigin(viewportSize: CGSize) -> CGPoint {
        let anchorX: CGFloat = vm.hasJoined ? CGFloat(vm.localX) : CGFloat((worldMin + worldMax) * 0.5)
        let anchorY: CGFloat = vm.hasJoined ? CGFloat(vm.localY) : CGFloat((worldMin + worldMax) * 0.5)
        let worldMinCG = CGFloat(worldMin)
        let worldMaxCG = CGFloat(worldMax)
        let viewHalfW = viewportSize.width * 0.5
        let viewHalfH = viewportSize.height * 0.5
        let softPadX = min(viewHalfW * 0.65, 140)
        let softPadY = min(viewHalfH * 0.65, 120)

        let minCamX = worldMinCG - softPadX
        let maxCamX = worldMaxCG - viewportSize.width + softPadX
        let minCamY = worldMinCG - softPadY
        let maxCamY = worldMaxCG - viewportSize.height + softPadY

        let camX = clamp(anchorX - viewHalfW, min: minCamX, max: maxCamX)
        let camY = clamp(anchorY - viewHalfH, min: minCamY, max: maxCamY)
        return CGPoint(x: camX, y: camY)
    }

    private func isWorldPointVisible(
        x: CGFloat,
        y: CGFloat,
        camX: CGFloat,
        camY: CGFloat,
        viewportWorldSize: CGSize,
        padding: CGFloat
    ) -> Bool {
        x >= camX - padding &&
        x <= camX + viewportWorldSize.width + padding &&
        y >= camY - padding &&
        y <= camY + viewportWorldSize.height + padding
    }

    private func countVisibleEffects(
        effects: [EffectManager.ActiveEffect],
        camX: CGFloat,
        camY: CGFloat,
        zoom: CGFloat,
        viewportSize: CGSize
    ) -> Int {
        let screenMargin: CGFloat = 42
        var count = 0
        for effect in effects {
            let screenX = (effect.x - camX) * zoom
            let screenY = (effect.y - camY) * zoom
            if screenX >= -screenMargin &&
                screenX <= viewportSize.width + screenMargin &&
                screenY >= -screenMargin &&
                screenY <= viewportSize.height + screenMargin {
                count += 1
            }
        }
        return count
    }
}

private struct ProceduralWorldBackdrop: View {
    let camX: CGFloat
    let camY: CGFloat
    let zoom: CGFloat

    var body: some View {
        Canvas(rendersAsynchronously: true) { ctx, size in
            let bgRect = CGRect(origin: .zero, size: size)
            ctx.fill(
                Path(bgRect),
                with: .linearGradient(
                    Gradient(stops: [
                        .init(color: Color(red: 0.09, green: 0.06, blue: 0.16), location: 0.0),
                        .init(color: Color(red: 0.05, green: 0.03, blue: 0.10), location: 0.45),
                        .init(color: Color(red: 0.02, green: 0.02, blue: 0.06), location: 1.0),
                    ]),
                    startPoint: CGPoint(x: size.width * 0.5, y: 0),
                    endPoint: CGPoint(x: size.width * 0.5, y: size.height)
                )
            )

            let worldRect = CGRect(
                x: CGFloat(worldMin),
                y: CGFloat(worldMin),
                width: CGFloat(worldMax - worldMin),
                height: CGFloat(worldMax - worldMin)
            )
            let tile: CGFloat = 32
            let worldViewportWidth = size.width / zoom
            let worldViewportHeight = size.height / zoom

            let minTileX = Int(floor(camX / tile))
            let maxTileX = Int(ceil((camX + worldViewportWidth) / tile))
            let minTileY = Int(floor(camY / tile))
            let maxTileY = Int(ceil((camY + worldViewportHeight) / tile))

            var lightTiles = Path()
            for ty in minTileY...maxTileY {
                for tx in minTileX...maxTileX where (tx + ty).isMultiple(of: 2) {
                    let r = CGRect(
                        x: (CGFloat(tx) * tile - camX) * zoom,
                        y: (CGFloat(ty) * tile - camY) * zoom,
                        width: tile * zoom,
                        height: tile * zoom
                    )
                    lightTiles.addRect(r)
                }
            }
            ctx.fill(lightTiles, with: .color(Color(red: 0.10, green: 0.08, blue: 0.18).opacity(0.80)))

            var minorGrid = Path()
            var majorGrid = Path()
            for tx in minTileX...maxTileX {
                let x = (CGFloat(tx) * tile - camX) * zoom
                if tx.isMultiple(of: 4) {
                    majorGrid.move(to: CGPoint(x: x, y: 0))
                    majorGrid.addLine(to: CGPoint(x: x, y: size.height))
                } else {
                    minorGrid.move(to: CGPoint(x: x, y: 0))
                    minorGrid.addLine(to: CGPoint(x: x, y: size.height))
                }
            }
            for ty in minTileY...maxTileY {
                let y = (CGFloat(ty) * tile - camY) * zoom
                if ty.isMultiple(of: 4) {
                    majorGrid.move(to: CGPoint(x: 0, y: y))
                    majorGrid.addLine(to: CGPoint(x: size.width, y: y))
                } else {
                    minorGrid.move(to: CGPoint(x: 0, y: y))
                    minorGrid.addLine(to: CGPoint(x: size.width, y: y))
                }
            }
            ctx.stroke(minorGrid, with: .color(Color(red: 0.18, green: 0.14, blue: 0.30).opacity(0.42)), lineWidth: 1)
            ctx.stroke(majorGrid, with: .color(Color(red: 0.28, green: 0.24, blue: 0.44).opacity(0.65)), lineWidth: 1.5)

            ctx.fill(
                Path(bgRect),
                with: .radialGradient(
                    Gradient(stops: [
                        .init(color: Color(red: 0.30, green: 0.20, blue: 0.45).opacity(0.20), location: 0.0),
                        .init(color: .clear, location: 1.0),
                    ]),
                    center: CGPoint(x: size.width * 0.5, y: size.height * 0.44),
                    startRadius: 0,
                    endRadius: max(size.width, size.height) * 0.6
                )
            )

            let borderRect = CGRect(
                x: (worldRect.minX - camX) * zoom,
                y: (worldRect.minY - camY) * zoom,
                width: worldRect.width * zoom,
                height: worldRect.height * zoom
            )
            ctx.stroke(
                Path(borderRect),
                with: .color(Color(red: 0.35, green: 0.78, blue: 1.0).opacity(0.35)),
                lineWidth: 6
            )
            ctx.stroke(
                Path(borderRect),
                with: .color(Color(red: 0.60, green: 0.86, blue: 1.0).opacity(0.85)),
                lineWidth: 2.5
            )
        }
    }
}

private func clamp(_ value: CGFloat, min minValue: CGFloat, max maxValue: CGFloat) -> CGFloat {
    guard maxValue >= minValue else { return minValue }
    return Swift.max(minValue, Swift.min(maxValue, value))
}


// MARK: - Subviews for rendering entities

private struct GameEntitiesCanvas: View {
    let players: [VisiblePlayerSnapshot]
    let weapons: [WeaponDrop]
    let t: TimeInterval
    let camX: CGFloat
    let camY: CGFloat
    let zoom: CGFloat

    var body: some View {
        Canvas(rendersAsynchronously: true) { ctx, _ in
            for weapon in weapons {
                let center = CGPoint(
                    x: (CGFloat(weapon.x) - camX) * zoom,
                    y: (CGFloat(weapon.y) - camY) * zoom
                )
                drawSword(in: &ctx, center: center, scale: zoom * 0.72, rotationDegrees: 12, glow: true)
            }

            for snapshot in players {
                let center = CGPoint(
                    x: (CGFloat(snapshot.worldX) - camX) * zoom,
                    y: (CGFloat(snapshot.worldY) - camY) * zoom
                )
                drawNinja(
                    in: &ctx,
                    center: center,
                    direction: snapshot.direction,
                    isMoving: snapshot.isMoving,
                    hitFlash: snapshot.isFlashing,
                    t: t,
                    scale: zoom,
                    baseColor: snapshot.color,
                    lowHealth: snapshot.player.health < 33
                )
                if snapshot.player.weaponCount > 0 {
                    forEachSwordPosition(count: Int(snapshot.player.weaponCount), t: t) { offset in
                        let orbitCenter = CGPoint(
                            x: center.x + offset.x * zoom,
                            y: center.y + offset.y * zoom
                        )
                        drawSword(
                            in: &ctx,
                            center: orbitCenter,
                            scale: zoom * 0.72,
                            rotationDegrees: -35,
                            glow: false
                        )
                    }
                }
            }
        }
    }

    private func drawNinja(
        in ctx: inout GraphicsContext,
        center: CGPoint,
        direction: NinjaGameViewModel.NinjaDirection,
        isMoving: Bool,
        hitFlash: Bool,
        t: TimeInterval,
        scale: CGFloat,
        baseColor: Color,
        lowHealth: Bool
    ) {
        let sprite = baseEntitySpriteSize * scale
        let w = sprite
        let h = sprite
        let origin = CGPoint(x: center.x - w * 0.5, y: center.y - h * 0.5)
        let tAdjusted = t * 1.5
        let bob = isMoving ? CGFloat(sin(tAdjusted * 4.0)) * 1.2 * scale : CGFloat(sin(tAdjusted * 1.6)) * 0.9 * scale
        let swing = isMoving ? CGFloat(sin(tAdjusted * 8.0)) * 3.5 * scale : CGFloat(sin(tAdjusted * 1.8)) * 0.8 * scale
        let legSwing = isMoving ? CGFloat(sin(tAdjusted * 8.0 + .pi / 2.0)) * 2.8 * scale : 0
        let top = origin.y + h * 0.10 + bob

        let primary = hitFlash ? Color.white : baseColor.opacity(lowHealth ? 0.92 : 1.0)
        let dark = hitFlash ? Color.white : Color(red: 0.04, green: 0.05, blue: 0.10)
        let hood = hitFlash ? Color.white : Color(red: 0.06, green: 0.08, blue: 0.14)
        let accent = hitFlash ? Color.white : Color(red: 0.85, green: 0.12, blue: 0.18)
        let skin = hitFlash ? Color.white : Color(red: 0.98, green: 0.82, blue: 0.72)
        let eye = hitFlash ? Color.white : Color(red: 0.60, green: 0.85, blue: 1.0)
        let facingEast = direction != .west

        func x(_ ratio: CGFloat) -> CGFloat { origin.x + ratio * w }
        func y(_ ratio: CGFloat) -> CGFloat { top + ratio * h }

        func tint(_ color: Color) -> Color {
            if hitFlash {
                return Color.white
            }
            return lowHealth ? color.opacity(0.85) : color
        }

        func fill(_ rect: CGRect, _ color: Color) {
            ctx.fill(Path(rect), with: .color(tint(color)))
        }

        let shadow = CGRect(x: x(0.22), y: y(0.75), width: w * 0.56, height: h * 0.10)
        ctx.fill(Path(ellipseIn: shadow), with: .color(Color.black.opacity(0.35)))

        // Head + mask
        fill(CGRect(x: x(0.30), y: y(-0.10), width: w * 0.40, height: h * 0.23), hood)
        fill(CGRect(x: x(0.28), y: y(-0.02), width: w * 0.44, height: h * 0.06), accent)
        fill(CGRect(x: x(0.30), y: y(0.03), width: w * 0.40, height: h * 0.10), hood)
        if direction == .north {
            fill(CGRect(x: x(0.35), y: y(0.05), width: w * 0.30, height: h * 0.02), eye.opacity(0.35))
        } else if facingEast {
            fill(CGRect(x: x(0.50), y: y(0.04), width: w * 0.18, height: h * 0.04), skin)
            fill(CGRect(x: x(0.60), y: y(0.032), width: w * 0.09, height: h * 0.046), eye.opacity(0.40))
        } else {
            fill(CGRect(x: x(0.34), y: y(0.04), width: w * 0.32, height: h * 0.05), skin)
            fill(CGRect(x: x(0.36), y: y(0.032), width: w * 0.10, height: h * 0.046), eye.opacity(0.38))
            fill(CGRect(x: x(0.54), y: y(0.032), width: w * 0.10, height: h * 0.046), eye.opacity(0.38))
        }

        // Torso + belt
        fill(CGRect(x: x(0.28), y: y(0.13), width: w * 0.44, height: h * 0.38), primary)
        fill(CGRect(x: x(0.27), y: y(0.44), width: w * 0.46, height: h * 0.07), dark)
        fill(CGRect(x: x(0.38), y: y(0.44), width: w * 0.15, height: h * 0.07), accent)

        // Arms
        fill(CGRect(x: x(0.16) - swing, y: y(0.15), width: w * 0.13, height: h * 0.28), primary)
        fill(CGRect(x: x(0.71) + swing - w * 0.13, y: y(0.15), width: w * 0.13, height: h * 0.28), primary)
        fill(CGRect(x: x(0.16) - swing, y: y(0.43), width: w * 0.10, height: h * 0.06), skin)
        fill(CGRect(x: x(0.74) + swing - w * 0.10, y: y(0.43), width: w * 0.10, height: h * 0.06), skin)

        // Legs + boots
        fill(CGRect(x: x(0.31) - legSwing, y: y(0.51), width: w * 0.15, height: h * 0.25), primary)
        fill(CGRect(x: x(0.54) + legSwing, y: y(0.51), width: w * 0.15, height: h * 0.25), primary)
        fill(CGRect(x: x(0.28) - legSwing, y: y(0.74), width: w * 0.20, height: h * 0.07), dark)
        fill(CGRect(x: x(0.52) + legSwing, y: y(0.74), width: w * 0.20, height: h * 0.07), dark)
    }

    private func drawSword(
        in ctx: inout GraphicsContext,
        center: CGPoint,
        scale: CGFloat,
        rotationDegrees: Double,
        glow: Bool
    ) {
        let size = 56 * scale
        let w = size
        let h = size
        let origin = CGPoint(x: center.x - w * 0.5, y: center.y - h * 0.5)
        let angle = CGFloat(rotationDegrees * .pi / 180.0)
        let c = cos(angle)
        let s = sin(angle)

        func rotatePoint(_ point: CGPoint) -> CGPoint {
            let dx = point.x - center.x
            let dy = point.y - center.y
            return CGPoint(
                x: center.x + dx * c - dy * s,
                y: center.y + dx * s + dy * c
            )
        }

        func rotatedRectPath(_ rect: CGRect) -> Path {
            var p = Path()
            let a = rotatePoint(CGPoint(x: rect.minX, y: rect.minY))
            let b = rotatePoint(CGPoint(x: rect.maxX, y: rect.minY))
            let c = rotatePoint(CGPoint(x: rect.maxX, y: rect.maxY))
            let d = rotatePoint(CGPoint(x: rect.minX, y: rect.maxY))
            p.move(to: a)
            p.addLine(to: b)
            p.addLine(to: c)
            p.addLine(to: d)
            p.closeSubpath()
            return p
        }

        let blade = CGRect(x: origin.x + w * 0.47, y: origin.y + h * 0.14, width: w * 0.08, height: h * 0.56)
        let edge = CGRect(x: origin.x + w * 0.52, y: origin.y + h * 0.16, width: w * 0.02, height: h * 0.52)
        let guardRect = CGRect(x: origin.x + w * 0.40, y: origin.y + h * 0.66, width: w * 0.22, height: h * 0.06)
        let grip = CGRect(x: origin.x + w * 0.46, y: origin.y + h * 0.71, width: w * 0.10, height: h * 0.15)
        let pommel = CGRect(x: origin.x + w * 0.44, y: origin.y + h * 0.85, width: w * 0.14, height: h * 0.06)

        if glow {
            let glowRect = CGRect(x: origin.x + w * 0.26, y: origin.y + h * 0.78, width: w * 0.48, height: h * 0.12)
            ctx.fill(Path(ellipseIn: glowRect), with: .color(Color(red: 0.45, green: 0.82, blue: 1.0).opacity(0.25)))
        }

        ctx.fill(rotatedRectPath(blade), with: .color(Color(red: 0.82, green: 0.90, blue: 1.0)))
        ctx.fill(rotatedRectPath(edge), with: .color(.white))
        ctx.fill(rotatedRectPath(guardRect), with: .color(Color(red: 0.90, green: 0.72, blue: 0.22)))
        ctx.fill(rotatedRectPath(grip), with: .color(Color(red: 0.25, green: 0.13, blue: 0.06)))
        ctx.fill(rotatedRectPath(pommel), with: .color(Color(red: 0.76, green: 0.58, blue: 0.15)))
    }
}

struct PlayerEntityView: View {
    let player: Player
    let vm: NinjaGameViewModel
    let t: TimeInterval
    let camX: CGFloat
    let camY: CGFloat
    let zoom: CGFloat
    
    @State private var hitFlashTime: TimeInterval = 0

    var body: some View {
        // Render local player from predicted local position, others from smoothed interpolation.
        let worldX: Float = {
            if player.id == vm.userId && vm.hasJoined { return vm.localX }
            return vm.smoothedPositions[player.id]?.x ?? player.x
        }()
        let worldY: Float = {
            if player.id == vm.userId && vm.hasJoined { return vm.localY }
            return vm.smoothedPositions[player.id]?.y ?? player.y
        }()
        
        let px = (CGFloat(worldX) - camX) * zoom
        let py = (CGFloat(worldY) - camY) * zoom

        // Render a fully procedural ninja (no texture assets).
        let direction = vm.playerDirections[player.id] ?? .south
        let isMoving = vm.playerIsMoving[player.id] ?? false
        let isFlashing = t - hitFlashTime < 0.15
        let baseColor = Color.fromId(player.id)

        playerSprite(direction: direction, isMoving: isMoving, t: t, isFlashing: isFlashing, color: baseColor)
        .shadow(color: Color.black.opacity(0.35), radius: 3, x: 0, y: 2)
        .colorMultiply(player.health < 33 ? Color.red.opacity(0.8) : Color.white)
        .onChange(of: player.health) { oldHealth, newHealth in
            if newHealth < oldHealth && newHealth > 0 {
                hitFlashTime = t
            }
        }
        .overlay(alignment: .top) {
            PlayerLabelsView(player: player)
                .offset(y: -46 * zoom)
        }
        .overlay {
            let swords = swordPositions(count: Int(player.weaponCount), t: t)
            ForEach(0..<swords.count, id: \.self) { i in
                ProceduralSwordSpriteView(
                    spriteSize: CGSize(width: 56 * zoom, height: 56 * zoom),
                    style: .orbit
                )
                .scaleEffect(0.72)
                .offset(x: swords[i].x * zoom, y: swords[i].y * zoom)
            }
        }
        .position(x: px, y: py)
    }

    private func playerSprite(direction: NinjaGameViewModel.NinjaDirection, isMoving: Bool, t: TimeInterval, isFlashing: Bool, color: Color) -> some View {
        ProceduralNinjaSpriteView(
            direction: direction,
            isMoving: isMoving,
            t: t,
            spriteSize: CGSize(width: 58 * zoom, height: 58 * zoom),
            hitFlash: isFlashing,
            baseColor: color
        )
    }
}

struct PlayerLabelsView: View {
    let player: Player

    var body: some View {
        VStack(spacing: 2) {
            if player.kills > 0 {
                Text("★ \(player.kills)")
                    .font(.system(size: 9, weight: .heavy, design: .rounded))
                    .foregroundStyle(SurvivorsTheme.accent)
            }
            Text(player.name.uppercased())
                .font(.system(size: 9, weight: .heavy, design: .rounded))
                .foregroundStyle(.white)
                .lineLimit(1)

            HealthBar(health: player.health)
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .background(Color(red: 0.06, green: 0.05, blue: 0.12).opacity(0.90))
        .overlay(Rectangle().strokeBorder(Color(white: 0.35), lineWidth: 1))
        .fixedSize()
    }
}

struct HealthBar: View {
    let health: UInt32

    private var healthFraction: CGFloat { CGFloat(min(100, health)) / 100.0 }
    private var barColor: Color {
        if health > 60 { return Color(red: 0.10, green: 0.90, blue: 0.20) }
        if health > 30 { return Color(red: 1.00, green: 0.75, blue: 0.00) }
        return Color(red: 0.95, green: 0.15, blue: 0.15)
    }

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Rectangle().fill(Color.black.opacity(0.60))
                Rectangle()
                    .fill(barColor)
                    .frame(width: max(2, geo.size.width * healthFraction))
            }
        }
        .frame(width: 38, height: 4)
        .overlay(Rectangle().strokeBorder(Color(white: 0.30), lineWidth: 1))
        .padding(.top, 1)
    }
}

struct WeaponEntityView: View {
    let weapon: WeaponDrop
    let camX: CGFloat
    let camY: CGFloat
    let zoom: CGFloat

    var body: some View {
        ProceduralSwordSpriteView(
            spriteSize: CGSize(width: 56 * zoom, height: 56 * zoom),
            style: .ground
        )
        .scaleEffect(0.72)
        .position(
            x: (CGFloat(weapon.x) - camX) * zoom,
            y: (CGFloat(weapon.y) - camY) * zoom
        )
    }
}

// MARK: - Native Sprite Rendering

struct ProceduralNinjaSpriteView: View {
    let direction: NinjaGameViewModel.NinjaDirection
    let isMoving: Bool
    let t: TimeInterval
    let spriteSize: CGSize
    let hitFlash: Bool
    let baseColor: Color

    var body: some View {
        ZStack {
            // Scarf rendered in a 4× wider canvas so the trail is never clipped.
            Canvas { ctx, size in
                guard size.width > 0, size.height > 0 else { return }
                let sw = size.width
                let h = size.height
                let bw = sw / 4.0              // logical body width
                let bodyLeft = (sw - bw) / 2.0 // body left edge in scarf-canvas space
                let tAdjusted = t * 1.5
                let bob = isMoving ? CGFloat(sin(tAdjusted * 4.0)) * 1.2 : CGFloat(sin(tAdjusted * 1.6)) * 0.9
                let top = h * 0.10 + bob

                let scarfRed = hitFlash ? Color.white : Color(red: 0.85, green: 0.12, blue: 0.18)
                // Trail direction: negative = left (east/south/north), positive = right (west)
                let trailSign: CGFloat = direction == .west ? 1.0 : -1.0
                let trailX = trailSign * (isMoving ? 70.0 : 30.0)
                let waveFreq = isMoving ? 10.0 : 3.5
                let waveAmp = isMoving ? 13.0 : 9.0

                var scarf = Path()
                let scarfBase = CGPoint(x: bodyLeft + bw * 0.45, y: top + h * 0.22)
                scarf.move(to: scarfBase)
                for i in 1...8 {
                    let seg = Double(i) / 8.0
                    let px = scarfBase.x + CGFloat(seg * Double(trailX) * Double(bw) * 0.01)
                    let py = scarfBase.y + CGFloat(sin(tAdjusted * waveFreq + seg * 6.0) * waveAmp * CGFloat(seg))
                    scarf.addLine(to: CGPoint(x: px, y: py))
                }
                ctx.stroke(scarf, with: .color(scarfRed),
                           style: StrokeStyle(lineWidth: 5 * (bw / 58), lineCap: .round, lineJoin: .round))
                ctx.stroke(scarf, with: .color(Color(red: 1.0, green: 0.55, blue: 0.60).opacity(0.45)),
                           style: StrokeStyle(lineWidth: 1.5 * (bw / 58), lineCap: .round, lineJoin: .round))
            }
            .frame(width: spriteSize.width * 4.0, height: spriteSize.height)

            Canvas(rendersAsynchronously: true) { ctx, size in
                guard size.width > 0, size.height > 0 else { return }

                var g = ctx
                var facing = direction
                if direction == .west {
                    g.translateBy(x: size.width, y: 0)
                    g.scaleBy(x: -1, y: 1)
                    facing = .east
                }

                let w = size.width
                let h = size.height
                let tAdjusted = t * 1.5 // Speed up for crisper anims
                let swing = isMoving ? CGFloat(sin(tAdjusted * 8.0)) * 3.5 : CGFloat(sin(tAdjusted * 1.8)) * 0.8
                let legSwing = isMoving ? CGFloat(sin(tAdjusted * 8.0 + .pi / 2.0)) * 2.8 : 0
                let bob = isMoving ? CGFloat(sin(tAdjusted * 4.0)) * 1.2 : CGFloat(sin(tAdjusted * 1.6)) * 0.9
                let top = h * 0.10 + bob

                // 1. Ambient Shadow
                let shadow = CGRect(x: w * 0.22, y: h * 0.85, width: w * 0.56, height: h * 0.10)
                g.fill(Path(ellipseIn: shadow), with: .color(Color.black.opacity(0.35)))

            // 3. Color Logic
            let primary = hitFlash ? Color.white : baseColor
            let highlight = hitFlash ? Color.white : primary.opacity(0.7)
            let dark = hitFlash ? Color.white : Color(red: 0.04, green: 0.05, blue: 0.10)
            let hoodColor = hitFlash ? Color.white : Color(red: 0.06, green: 0.08, blue: 0.14)
            let accentColor = hitFlash ? Color.white : Color(red: 0.85, green: 0.12, blue: 0.18)
            let skinColor = hitFlash ? Color.white : Color(red: 0.98, green: 0.82, blue: 0.72)
            let eyeGlow = hitFlash ? Color.white : Color(red: 0.60, green: 0.85, blue: 1.0)

            func fillRect(_ x: CGFloat, _ y: CGFloat, _ width: CGFloat, _ height: CGFloat, _ color: Color) {
                g.fill(Path(CGRect(x: x, y: y, width: width, height: height)), with: .color(color))
            }

            // 4. Head Remastered
            // Hood peak
            var headPath = Path()
            headPath.move(to: CGPoint(x: w * 0.30, y: top + h * 0.23))
            headPath.addLine(to: CGPoint(x: w * 0.50, y: top - h * 0.02)) // Peak
            headPath.addLine(to: CGPoint(x: w * 0.70, y: top + h * 0.23))
            g.fill(headPath, with: .color(hoodColor))
            
            fillRect(w * 0.30, top, w * 0.40, h * 0.23, hoodColor)
            fillRect(w * 0.45, top + h * 0.01, w * 0.10, h * 0.10, highlight.opacity(0.09)) // Hood center highlight
            fillRect(w * 0.31, top + h * 0.02, w * 0.38, h * 0.05, highlight.opacity(0.4)) // Peak hilight
            fillRect(w * 0.28, top + h * 0.08, w * 0.44, h * 0.06, accentColor) // Headband
            fillRect(w * 0.30, top + h * 0.09, w * 0.10, h * 0.02, Color.white.opacity(0.22)) // Headband glint
            fillRect(w * 0.30, top + h * 0.13, w * 0.40, h * 0.10, hoodColor) // Bottom face
            fillRect(w * 0.30, top + h * 0.21, w * 0.40, h * 0.02, dark.opacity(0.28)) // Hood bottom shadow

            if facing == .north {
                fillRect(w * 0.35, top + h * 0.15, w * 0.30, h * 0.02, highlight.opacity(0.2))
            } else if facing == .east {
                fillRect(w * 0.50, top + h * 0.14, w * 0.18, h * 0.04, skinColor)
                fillRect(w * 0.60, top + h * 0.132, w * 0.09, h * 0.046, eyeGlow.opacity(0.40)) // Eye glow
                fillRect(w * 0.62, top + h * 0.14, w * 0.05, h * 0.03, dark) // Eye
            } else {
                // Front eyes & skin
                fillRect(w * 0.34, top + h * 0.14, w * 0.32, h * 0.05, skinColor)
                fillRect(w * 0.36, top + h * 0.132, w * 0.10, h * 0.046, eyeGlow.opacity(0.38)) // Left eye glow
                fillRect(w * 0.54, top + h * 0.132, w * 0.10, h * 0.046, eyeGlow.opacity(0.38)) // Right eye glow
                fillRect(w * 0.38, top + h * 0.14, w * 0.06, h * 0.03, dark) // Left eye
                fillRect(w * 0.56, top + h * 0.14, w * 0.06, h * 0.03, dark) // Right eye
            }

            // 5. Torso & Sash
            fillRect(w * 0.28, top + h * 0.23, w * 0.44, h * 0.38, primary)
            fillRect(w * 0.47, top + h * 0.23, w * 0.06, h * 0.38, dark.opacity(0.4)) // Mid slit
            fillRect(w * 0.28, top + h * 0.23, w * 0.09, h * 0.31, dark.opacity(0.22)) // Left torso shadow
            fillRect(w * 0.63, top + h * 0.23, w * 0.09, h * 0.31, highlight.opacity(0.14)) // Right rim light
            fillRect(w * 0.27, top + h * 0.54, w * 0.46, h * 0.07, dark) // Belt (Obi)
            fillRect(w * 0.38, top + h * 0.54, w * 0.15, h * 0.07, accentColor) // Knot
            fillRect(w * 0.27, top + h * 0.54, w * 0.46, h * 0.01, highlight.opacity(0.18)) // Belt top highlight
            fillRect(w * 0.37, top + h * 0.61, w * 0.06, h * 0.03, accentColor.opacity(0.85)) // Knot left tail
            fillRect(w * 0.47, top + h * 0.61, w * 0.06, h * 0.03, accentColor.opacity(0.85)) // Knot right tail

            // 6. Arms remastered
            fillRect(w * 0.16 - swing, top + h * 0.25, w * 0.13, h * 0.28, primary)
            fillRect(w * 0.71 + swing - w * 0.13, top + h * 0.25, w * 0.13, h * 0.28, primary)
            fillRect(w * 0.16 - swing, top + h * 0.25, w * 0.04, h * 0.28, highlight.opacity(0.3)) // Rim light
            // Forearm wraps
            fillRect(w * 0.16 - swing, top + h * 0.37, w * 0.13, h * 0.017, dark.opacity(0.30))
            fillRect(w * 0.16 - swing, top + h * 0.43, w * 0.13, h * 0.017, dark.opacity(0.30))
            fillRect(w * 0.71 + swing - w * 0.13, top + h * 0.37, w * 0.13, h * 0.017, dark.opacity(0.30))
            fillRect(w * 0.71 + swing - w * 0.13, top + h * 0.43, w * 0.13, h * 0.017, dark.opacity(0.30))

            fillRect(w * 0.16 - swing, top + h * 0.53, w * 0.10, h * 0.06, skinColor) // Hands
            fillRect(w * 0.74 + swing - w * 0.10, top + h * 0.53, w * 0.10, h * 0.06, skinColor)

            // 7. Legs remastered
            fillRect(w * 0.31 - legSwing, top + h * 0.61, w * 0.15, h * 0.25, primary)
            fillRect(w * 0.54 + legSwing, top + h * 0.61, w * 0.15, h * 0.25, primary)
            fillRect(w * 0.28 - legSwing, top + h * 0.84, w * 0.20, h * 0.07, dark) // Boots
            fillRect(w * 0.52 + legSwing, top + h * 0.84, w * 0.20, h * 0.07, dark)
            // Shin definition
            fillRect(w * 0.37 - legSwing, top + h * 0.62, w * 0.03, h * 0.22, dark.opacity(0.22))
            fillRect(w * 0.60 + legSwing, top + h * 0.62, w * 0.03, h * 0.22, dark.opacity(0.22))
            // Boot toe highlights
            fillRect(w * 0.31 - legSwing, top + h * 0.84, w * 0.08, h * 0.020, highlight.opacity(0.14))
            fillRect(w * 0.55 + legSwing, top + h * 0.84, w * 0.08, h * 0.020, highlight.opacity(0.14))
            }
            .frame(width: spriteSize.width, height: spriteSize.height)
        }
        .frame(width: spriteSize.width, height: spriteSize.height)
        .accessibilityLabel("Procedural ninja sprite")
    }
}

struct ProceduralSwordSpriteView: View {
    enum Style {
        case orbit
        case ground
    }

    let spriteSize: CGSize
    let style: Style

    var body: some View {
        Canvas(rendersAsynchronously: true) { ctx, size in
            guard size.width > 0, size.height > 0 else { return }

            let w = size.width
            let h = size.height
            let blade = CGRect(x: w * 0.47, y: h * 0.14, width: w * 0.08, height: h * 0.56)
            let edge = CGRect(x: w * 0.52, y: h * 0.16, width: w * 0.02, height: h * 0.52)
            let guardRect = CGRect(x: w * 0.40, y: h * 0.66, width: w * 0.22, height: h * 0.06)
            let grip = CGRect(x: w * 0.46, y: h * 0.71, width: w * 0.10, height: h * 0.15)
            let pommel = CGRect(x: w * 0.44, y: h * 0.85, width: w * 0.14, height: h * 0.06)

            if style == .ground {
                let glow = CGRect(x: w * 0.26, y: h * 0.78, width: w * 0.48, height: h * 0.12)
                ctx.fill(Path(ellipseIn: glow), with: .color(Color(red: 0.45, green: 0.82, blue: 1.0).opacity(0.25)))
            }

            ctx.fill(Path(blade), with: .color(Color(red: 0.82, green: 0.90, blue: 1.0)))
            ctx.fill(Path(edge), with: .color(.white))
            ctx.fill(Path(guardRect), with: .color(Color(red: 0.90, green: 0.72, blue: 0.22)))
            ctx.fill(Path(grip), with: .color(Color(red: 0.25, green: 0.13, blue: 0.06)))
            ctx.fill(Path(pommel), with: .color(Color(red: 0.76, green: 0.58, blue: 0.15)))
        }
        .frame(width: spriteSize.width, height: spriteSize.height)
        .rotationEffect(style == .orbit ? .degrees(-35) : .degrees(12))
        .accessibilityLabel("Procedural sword sprite")
    }
}

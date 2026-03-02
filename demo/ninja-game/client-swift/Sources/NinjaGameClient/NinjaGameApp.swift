import SwiftUI
import SpacetimeDB
import Observation
@preconcurrency import AVFoundation
#if canImport(AppKit)
import AppKit
#endif

enum SurvivorsTheme {
    static let accent = Color(red: 0.28, green: 0.58, blue: 0.98)
    static let panelStroke = Color.white.opacity(0.16)
    static let backdropTop = Color(red: 0.06, green: 0.08, blue: 0.16)
    static let backdropBottom = Color(red: 0.10, green: 0.12, blue: 0.23)
    static let backdropGlow = Color(red: 0.24, green: 0.45, blue: 0.95).opacity(0.26)
    static let backdropGlowSecondary = Color(red: 0.10, green: 0.72, blue: 0.92).opacity(0.16)
}

// MARK: - Shared UI Style

extension View {
    func pixelPanel() -> some View {
        background(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(.ultraThinMaterial)
                .overlay(
                    RoundedRectangle(cornerRadius: 18, style: .continuous)
                        .strokeBorder(SurvivorsTheme.panelStroke, lineWidth: 1)
                )
        )
    }
}

struct PixelButtonStyle: ButtonStyle {
    var filled: Bool = false
    var danger: Bool = false
    var accentColor: Color = SurvivorsTheme.accent

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold, design: .rounded))
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .foregroundStyle(fgColor)
            .background {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(bgColor(configuration.isPressed))
            }
            .overlay {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(borderColor, lineWidth: 1)
            }
            .shadow(color: Color.black.opacity(configuration.isPressed ? 0.04 : 0.10), radius: 4, x: 0, y: 1)
            .scaleEffect(configuration.isPressed ? 0.985 : 1.0)
            .animation(.easeInOut(duration: 0.10), value: configuration.isPressed)
    }

    private var fgColor: Color {
        if danger && filled {
            return .white
        }
        if danger {
            return Color(red: 1.0, green: 0.42, blue: 0.42)
        }
        return filled ? .white : .primary
    }

    private func bgColor(_ pressed: Bool) -> Color {
        if danger && filled {
            return Color(red: 0.85, green: 0.20, blue: 0.22).opacity(pressed ? 0.82 : 0.96)
        }
        if danger {
            return Color(red: 0.85, green: 0.20, blue: 0.22).opacity(pressed ? 0.18 : 0.10)
        }
        if filled {
            return accentColor.opacity(pressed ? 0.82 : 0.95)
        }
        return Color.white.opacity(pressed ? 0.16 : 0.10)
    }

    private var borderColor: Color {
        if danger {
            return Color(red: 1.0, green: 0.42, blue: 0.42).opacity(0.75)
        }
        if filled {
            return accentColor.opacity(0.86)
        }
        return Color.white.opacity(0.24)
    }
}

struct SurvivorsChipBackground: View {
    var cornerRadius: CGFloat = 6

    var body: some View {
        SurvivorsShapeSurface(
            shape: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous),
            fallbackMaterial: .regularMaterial,
            usesLiquidGlass: true
        )
    }
}

struct SurvivorsPanelBackground: View {
    var cornerRadius: CGFloat = 18

    var body: some View {
        // Keep large panels deterministic to avoid liquid geometry morphing.
        SurvivorsShapeSurface(
            shape: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous),
            fallbackMaterial: .ultraThinMaterial,
            stroke: SurvivorsTheme.panelStroke,
            lineWidth: 1,
            usesLiquidGlass: false
        )
    }
}



struct SurvivorsShapeSurface<S: InsettableShape>: View {
    let shape: S
    var fallbackMaterial: Material
    var stroke: Color? = nil
    var lineWidth: CGFloat = 1
    var usesLiquidGlass: Bool = true

    var body: some View {
        liquidOrFallback
            .overlay {
                if let stroke {
                    shape.strokeBorder(stroke, lineWidth: lineWidth)
                }
            }
    }

    @ViewBuilder
    private var liquidOrFallback: some View {
        if usesLiquidGlass, #available(macOS 26.0, iOS 26.0, *) {
            shape
                .fill(.clear)
                .glassEffect()
                .clipShape(shape)
        } else {
            shape.fill(fallbackMaterial)
        }
    }
}

struct SurvivorsBackdrop: View {
    // Deterministic star field using golden-ratio hashing.
    private static let stars: [(x: Double, y: Double, sz: CGFloat, spd: Double, ph: Double)] = (0..<60).map { i in
        let g = 0.6180339887498949
        return (
            x: (Double(i) * g).truncatingRemainder(dividingBy: 1.0),
            y: (Double(i * 7 + 3) * g).truncatingRemainder(dividingBy: 1.0),
            sz: CGFloat(1.5 + (Double(i * 13 + 7) * g).truncatingRemainder(dividingBy: 1.0) * 2.0),
            spd: 0.4 + (Double(i * 19 + 11) * g).truncatingRemainder(dividingBy: 1.0) * 1.4,
            ph: (Double(i * 31 + 17) * g).truncatingRemainder(dividingBy: 1.0) * .pi * 2
        )
    }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1.0 / 30.0)) { timeline in
            let t = timeline.date.timeIntervalSinceReferenceDate
            let glowAX = 0.50 + 0.28 * cos(t * 0.07)
            let glowAY = 0.38 + 0.20 * sin(t * 0.05)
            let glowBX = 0.52 + 0.24 * sin(t * 0.06 + 1.3)
            let glowBY = 0.62 + 0.20 * cos(t * 0.05 + 0.8)

            ZStack {
                LinearGradient(
                    colors: [SurvivorsTheme.backdropTop, SurvivorsTheme.backdropBottom],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )

                // Slow-scrolling pixel grid
                Canvas { ctx, size in
                    let grid: CGFloat = 64
                    let xShift = CGFloat((t * 4).truncatingRemainder(dividingBy: Double(grid)))
                    let yShift = CGFloat((t * 3).truncatingRemainder(dividingBy: Double(grid)))
                    var path = Path()

                    var x = -grid + xShift
                    while x <= size.width + grid {
                        path.move(to: CGPoint(x: x, y: 0))
                        path.addLine(to: CGPoint(x: x, y: size.height))
                        x += grid
                    }

                    var y = -grid + yShift
                    while y <= size.height + grid {
                        path.move(to: CGPoint(x: 0, y: y))
                        path.addLine(to: CGPoint(x: size.width, y: y))
                        y += grid
                    }

                    ctx.stroke(path, with: .color(Color.purple.opacity(0.12)), lineWidth: 1)
                }

                // Twinkling pixel stars
                Canvas { ctx, size in
                    for star in Self.stars {
                        let alpha = 0.15 + 0.85 * (0.5 + 0.5 * sin(t * star.spd + star.ph))
                        let rect = CGRect(
                            x: star.x * size.width - star.sz / 2,
                            y: star.y * size.height - star.sz / 2,
                            width: star.sz, height: star.sz
                        )
                        ctx.fill(Path(rect), with: .color(Color.white.opacity(alpha)))
                    }
                }

                // Purple ambient glow
                RadialGradient(
                    colors: [SurvivorsTheme.backdropGlow, .clear],
                    center: UnitPoint(x: glowAX, y: glowAY),
                    startRadius: 30,
                    endRadius: 540
                )
                .blur(radius: 40)

                // Crimson ambient glow
                RadialGradient(
                    colors: [SurvivorsTheme.backdropGlowSecondary, .clear],
                    center: UnitPoint(x: glowBX, y: glowBY),
                    startRadius: 24,
                    endRadius: 440
                )
                .blur(radius: 30)
            }
        }
        .ignoresSafeArea()
    }
}

extension View {
    func survivorsPanel(cornerRadius: CGFloat = 18) -> some View {
        background(SurvivorsPanelBackground(cornerRadius: cornerRadius))
    }

    func survivorsShadow() -> some View {
        shadow(color: Color.black.opacity(0.15), radius: 10, x: 0, y: 5)
    }
}

// MARK: - macOS lifecycle

#if canImport(AppKit)
@MainActor
private final class NinjaGameAppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationWillTerminate(_ notification: Notification) {
        if let client = SpacetimeClient.shared {
            client.disconnect()
            SpacetimeClient.shared = nil
        }
    }
}
#endif

// MARK: - Entry point

@main
struct NinjaGameApp: App {
    #if canImport(AppKit)
    @NSApplicationDelegateAdaptor(NinjaGameAppDelegate.self) private var appDelegate
    #endif

    init() {
        #if canImport(AppKit)
        NSApplication.shared.setActivationPolicy(.regular)
        #endif
    }

    var body: some Scene {
        WindowGroup("SpaceTimeDB Survivors") {
            RootView()
                .frame(minWidth: 700, minHeight: 560)
        }
        .windowStyle(.titleBar)
    }
}

// MARK: - App-level state machine

private enum Screen {
    case title      // main menu + name entry
    case lobbyBrowser // looking for a game
    case lobby      // waiting for match to start
    case playing    // full game
}

// MARK: - Root View Model

/// We need a global view model that connects on start, and lives across
/// the Lobby and Playing screens, instead of tying it to NinjaGameView.
@MainActor
@Observable
final class RootViewModel {
    let audio = MusicPlayer()
    var gameVM = NinjaGameViewModel()
}

// MARK: - Root view

struct RootView: View {
    @State private var screen: Screen = .title
    @State private var playerName: String = "Player \(Int.random(in: 1...99))"
    @State private var titleOpacity = 0.0
    
    @State private var vm = RootViewModel()

    var body: some View {
        ZStack {
            if screen != .playing {
                SurvivorsBackdrop()
            }

            switch screen {
            case .title:
                TitleView(
                    titleOpacity: titleOpacity,
                    vm: vm.gameVM,
                    onBrowseLobbies: {
                        vm.gameVM.initialName = playerName
                        vm.gameVM.clearPendingQuickJoinFromTitle()
                        vm.gameVM.start()
                        withAnimation(.easeIn(duration: 0.35)) { screen = .lobbyBrowser }
                    },
                    onQuickJoin: {
                        vm.gameVM.initialName = playerName
                        vm.gameVM.scheduleQuickJoinFromTitle()
                        vm.gameVM.start()
                    },
                    playerName: $playerName,
                    selectedEnvironment: $vm.gameVM.environment
                )
                    .transition(.opacity)
            
            case .lobbyBrowser:
                LobbyBrowserView(vm: vm.gameVM) { action in
                    switch action {
                    case .resetName:
                        vm.gameVM.stop()
                        withAnimation { screen = .title }
                    case .quit:
                        vm.gameVM.stop()
                        withAnimation { screen = .title }
                    }
                }
                .transition(.asymmetric(
                    insertion: .opacity.combined(with: .scale(scale: 1.04)),
                    removal: .opacity
                ))
            
            case .lobby:
                LobbyView(vm: vm.gameVM) { action in
                    switch action {
                    case .resetName:
                        vm.gameVM.stop()
                        withAnimation { screen = .title }
                    case .quit:
                        vm.gameVM.stop()
                        withAnimation { screen = .title }
                    }
                }
                .transition(.asymmetric(
                    insertion: .opacity.combined(with: .scale(scale: 1.04)),
                    removal: .opacity
                ))

            case .playing:
                NinjaGameView(
                    isBackground: false,
                    isMuted: vm.audio.isMuted,
                    injectedVM: vm.gameVM,
                    onMuteToggle: { vm.audio.toggleMute() }
                ) { action in
                    switch action {
                    case .resetName:
                        vm.gameVM.stop()
                        withAnimation { screen = .title }
                    case .quit:
                        vm.gameVM.stop()
                        withAnimation { screen = .title }
                    }
                } onMusicChange: { playInGameMusic in
                    if playInGameMusic {
                        vm.audio.crossfadeToGame()
                    } else {
                        vm.audio.switchToTitleMusic()
                    }
                }
                .transition(.opacity)
            }
        }
        .tint(SurvivorsTheme.accent)
        .animation(.easeInOut(duration: 0.5), value: screen)
        .onAppear {
            vm.audio.playTitle()
            withAnimation(.easeIn(duration: 1.4)) { titleOpacity = 1.0 }
        }
        .onChange(of: screen) { _, newScreen in
            // Any screen outside active play uses title music.
            if newScreen != .playing {
                vm.audio.switchToTitleMusic()
            }
        }
        .onChange(of: vm.gameVM.activeLobbyId) { _, newLobbyId in
            if newLobbyId != nil && (screen == .lobbyBrowser || screen == .title) {
                if vm.gameVM.isQuickJoinActive {
                    vm.gameVM.isQuickJoinActive = false
                    if !vm.gameVM.isPlaying {
                        SoundEffects.shared.play(.enterArena)
                        StartMatch.invoke()
                    }
                    withAnimation(.easeIn(duration: 0.35)) { screen = .playing }
                } else if vm.gameVM.isPlaying {
                    SoundEffects.shared.play(.enterArena)
                    withAnimation(.easeIn(duration: 0.35)) { screen = .playing }
                } else {
                    withAnimation(.easeIn(duration: 0.35)) { screen = .lobby }
                }
            } else if newLobbyId == nil && (screen == .lobby || screen == .playing) {
                withAnimation(.easeIn(duration: 0.35)) { screen = .lobbyBrowser }
            }
        }
        .onChange(of: vm.gameVM.isPlaying) { _, isPlaying in
            // Auto transition based on backend Lobby.is_playing
            if isPlaying && screen == .lobby {
                SoundEffects.shared.play(.enterArena)
                withAnimation(.easeIn(duration: 0.35)) { screen = .playing }
            } else if !isPlaying && screen == .playing {
                withAnimation(.easeIn(duration: 0.35)) { screen = .lobby }
            }
        }
    }
}

// MARK: - Title screen

struct TitleView: View {
    let titleOpacity: Double
    var vm: NinjaGameViewModel
    let onBrowseLobbies: () -> Void
    let onQuickJoin: () -> Void
    @Binding var playerName: String
    @Binding var selectedEnvironment: SpacetimeEnvironment

    @State private var pulsePlay = false
    @State private var isConnecting = false

    private var trimmedName: String {
        playerName.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canStart: Bool { !trimmedName.isEmpty }

    private var endpointLabel: String {
        switch selectedEnvironment {
        case .local: return "127.0.0.1:3000"
        case .prod: return "maincloud.spacetimedb.com"
        }
    }

    var body: some View {
        GeometryReader { geo in
            ScrollView(showsIndicators: false) {
                VStack(spacing: 0) {
                    Spacer(minLength: 20)

                    // ── Title ──
                    VStack(spacing: 8) {
                        let logoSize = min(geo.size.width * 0.28, 180.0)
                        Image("spacetime_logo", bundle: .module)
                            .resizable()
                            .scaledToFit()
                            .frame(width: logoSize, height: logoSize)
                            .shadow(color: .black.opacity(0.18), radius: 6, x: 0, y: 3)

                        Text("Ninja Wars")
                            .font(.system(size: 42, weight: .heavy, design: .rounded))
                            .foregroundColor(Color(red: 0.90, green: 0.95, blue: 1.0))
                            .shadow(color: Color(red: 0.2, green: 0.4, blue: 0.8).opacity(0.45), radius: 8, x: 0, y: 2)

                        Text("Realtime multiplayer on SpacetimeDB")
                            .font(.system(size: 12, weight: .medium, design: .rounded))
                            .foregroundStyle(Color(white: 0.72))
                            .padding(.top, 8)
                    }
                    .multilineTextAlignment(.center)
                    .minimumScaleFactor(0.5)
                    .opacity(titleOpacity)
                    .padding(.bottom, 44)

                    // ── Controls ──
                    VStack(spacing: 14) {
                        // Name input
                        TextField("Enter your ninja name…", text: $playerName)
                            .textFieldStyle(.plain)
                            .font(.system(size: 16, weight: .bold, design: .rounded))
                            .foregroundColor(.white)
                            .padding(.horizontal, 14)
                            .padding(.vertical, 14)
                            .background(Color.white.opacity(0.06))
                            .overlay(Rectangle().strokeBorder(Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.40), lineWidth: 2))
                            .onSubmit {
                                guard canStart else { return }
                                SoundEffects.shared.play(.buttonPress)
                                onQuickJoin()
                            }

                        // Environment picker toggle
                        HStack(spacing: 0) {
                            ForEach(SpacetimeEnvironment.allCases) { env in
                                let isSelected = selectedEnvironment == env
                                Button {
                                    SoundEffects.shared.play(.buttonPress)
                                    selectedEnvironment = env
                                } label: {
                                    Text(env.rawValue)
                                        .font(.system(size: 12, weight: .semibold, design: .rounded))
                                        .frame(maxWidth: .infinity)
                                        .padding(.vertical, 8)
                                        .foregroundColor(isSelected ? .white : Color(white: 0.85))
                                        .background(isSelected ? SurvivorsTheme.accent.opacity(0.92) : Color.clear)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .background(Color.white.opacity(0.08))
                        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.white.opacity(0.22), lineWidth: 1))
                        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                        .padding(.horizontal, 48)

                        Text(endpointLabel)
                            .font(.system(size: 10, weight: .medium, design: .rounded))
                            .foregroundStyle(Color(white: 0.62))
                            .padding(.top, -6)

                        // ── PLAY NOW ──
                        Button {
                            guard canStart, !isConnecting else { return }
                            SoundEffects.shared.play(.buttonPress)
                            isConnecting = true
                            onQuickJoin()
                        } label: {
                            HStack(spacing: 8) {
                                if isConnecting {
                                    ProgressView().controlSize(.small).tint(.black)
                                    Text("Connecting...")
                                        .font(.system(size: 16, weight: .semibold, design: .rounded))
                                } else {
                                    Image(systemName: "star.fill")
                                    Text("Quick Play")
                                        .font(.system(size: 16, weight: .semibold, design: .rounded))
                                }
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 6)
                        }
                        .buttonStyle(PixelButtonStyle(filled: true))
                        .disabled(!canStart || isConnecting)
                        .opacity(canStart ? 1.0 : 0.4)
                        .keyboardShortcut(.defaultAction)
                        .padding(.top, 4)

                        if isConnecting && !vm.connectionDetail.isEmpty {
                            Text(vm.connectionDetail)
                                .font(.system(size: 11, design: .rounded))
                                .foregroundStyle(.white.opacity(0.80))
                                .padding(.top, -6)
                        }

                        if isConnecting {
                            Button("Cancel") {
                                SoundEffects.shared.play(.buttonPress)
                                isConnecting = false
                                vm.stop()
                            }
                            .buttonStyle(PixelButtonStyle())
                            .padding(.bottom, 6)
                        }

                        // ── Browse Lobbies ──
                        Button {
                            SoundEffects.shared.play(.buttonPress)
                            onBrowseLobbies()
                        } label: {
                            HStack(spacing: 8) {
                                Image(systemName: "person.3.fill")
                                Text("Browse Lobbies")
                                    .font(.system(size: 14, weight: .semibold, design: .rounded))
                            }
                            .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(PixelButtonStyle())
                        .disabled(!canStart || isConnecting)
                        .opacity(canStart && !isConnecting ? 1.0 : 0.4)

                        // ── Utility row ──
                        HStack(spacing: 14) {
                            Button(action: clearServer) {
                                Text("Clear Server")
                                    .font(.system(size: 11, weight: .semibold, design: .rounded))
                                    .foregroundColor(.red.opacity(0.8))
                            }
                            .buttonStyle(.plain)

                            Text("·").foregroundColor(Color(white: 0.25))

                            Button(action: quitApplication) {
                                Text("Quit")
                                    .font(.system(size: 11, weight: .semibold, design: .rounded))
                                    .foregroundColor(Color(white: 0.45))
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(.top, 6)
                    }
                    .frame(width: 380)
                    .opacity(titleOpacity)
                    .padding(.bottom, 32)

                    Text("Realtime multiplayer powered by SpacetimeDB")
                        .font(.system(size: 11, design: .rounded))
                        .foregroundStyle(.white.opacity(0.35))

                    Spacer(minLength: 20)
                }
                .frame(maxWidth: .infinity, minHeight: geo.size.height, alignment: .center)
            }
        }
        .onAppear {
            withAnimation(.easeInOut(duration: 1.8).repeatForever(autoreverses: true)) {
                pulsePlay = true
            }
        }
    }

    private func quitApplication() {
        #if canImport(AppKit)
        NSApplication.shared.terminate(nil)
        #endif
    }

    private func clearServer() {
        SoundEffects.shared.play(.menuButton)
        ClearServer.invoke()
    }
}



// MARK: - Lobby Browser Screen

struct LobbyBrowserView: View {
    let vm: NinjaGameViewModel
    let onAction: (ExitAction) -> Void
    
    @State private var newLobbyName: String = ""
    @State private var showingCreateForm = false

    var body: some View {
        ZStack {
            VStack(spacing: 24) {
                HStack(alignment: .bottom) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Lobbies")
                            .font(.system(size: 26, weight: .heavy, design: .rounded))
                            .foregroundStyle(.white)

                        HStack(spacing: 6) {
                            Rectangle()
                                .fill(vm.isConnected ? Color.green : Color.orange)
                                .frame(width: 8, height: 8)
                            Text(vm.isConnected
                                 ? "Online · \(vm.myPlayer?.name ?? "Joining...")"
                                 : (vm.connectionDetail.isEmpty ? "Connecting..." : vm.connectionDetail))
                                .font(.system(size: 11, weight: .medium, design: .rounded))
                                .foregroundStyle(vm.isConnected ? Color(white: 0.65) : .orange)
                        }
                    }
                    
                    Spacer()
                    
                    Button(action: { vm.refreshLobbies() }) {
                        Text("Refresh")
                    }
                    .buttonStyle(PixelButtonStyle())
                    .disabled(!vm.isConnected)
                }

                if showingCreateForm {
                    VStack(spacing: 12) {
                        HStack {
                            Text("Create Lobby")
                                .font(.system(size: 13, weight: .semibold, design: .rounded))
                                .foregroundStyle(.white)
                            Spacer()
                        }

                        TextField("Lobby name", text: $newLobbyName)
                            .textFieldStyle(.plain)
                            .font(.system(size: 14, weight: .medium, design: .rounded))
                            .foregroundColor(.white)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 9)
                            .background(Color.white.opacity(0.06))
                            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.white.opacity(0.24), lineWidth: 1))
                            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))

                        HStack(spacing: 10) {
                            Button("Cancel") {
                                withAnimation { showingCreateForm = false }
                            }
                            .buttonStyle(PixelButtonStyle())
                            .frame(maxWidth: .infinity)

                            Button("Create Lobby") {
                                SoundEffects.shared.play(.enterArena)
                                vm.isQuickJoinActive = false
                                vm.createLobbyWithRetry(name: newLobbyName)
                                withAnimation { showingCreateForm = false }
                            }
                            .buttonStyle(PixelButtonStyle(filled: true))
                            .disabled(newLobbyName.isEmpty)
                            .frame(maxWidth: .infinity)
                        }
                    }
                    .padding(16)
                    .background(Color.white.opacity(0.05))
                    .overlay(Rectangle().strokeBorder(Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.30), lineWidth: 2))
                }

                VStack(spacing: 0) {
                    HStack {
                        Text("Available Lobbies")
                            .font(.system(size: 10, weight: .semibold, design: .rounded))
                            .foregroundStyle(Color(white: 0.40))
                        Spacer()
                        Text("\(vm.lobbies.count) / 50")
                            .font(.system(size: 10, weight: .medium, design: .rounded).monospacedDigit())
                            .foregroundStyle(Color(white: 0.30))
                    }
                    .padding(.horizontal, 4)
                    .padding(.bottom, 8)

                    ScrollView {
                        VStack(spacing: 8) {
                            if vm.lobbies.isEmpty {
                                VStack(spacing: 8) {
                                    Text("(zzz)")
                                    .font(.system(size: 22, weight: .heavy, design: .rounded))
                                        .foregroundStyle(Color(white: 0.25))
                                    Text("No lobbies active")
                                        .font(.system(size: 11, weight: .medium, design: .rounded))
                                        .foregroundStyle(Color(white: 0.30))
                                }
                                .padding(.vertical, 40)
                                .frame(maxWidth: .infinity)
                            } else {
                                ForEach(vm.lobbies, id: \.id) { lobby in
                                    let lobbyPlayerCount = vm.playerCount(forLobbyId: lobby.id)
                                    let isFull = lobbyPlayerCount >= NinjaGameViewModel.maxPlayersPerLobby
                                    HStack {
                                        VStack(alignment: .leading, spacing: 4) {
                                            Text(lobby.name)
                                                .font(.system(size: 13, weight: .semibold, design: .rounded))
                                                .foregroundStyle(.white)

                                            HStack(spacing: 10) {
                                                Text(lobby.isPlaying ? "Playing" : "Waiting")
                                                    .foregroundStyle(lobby.isPlaying ? .orange : .green)
                                                Text("\(lobbyPlayerCount)/\(NinjaGameViewModel.maxPlayersPerLobby)")
                                                    .foregroundStyle(isFull ? .red : Color(white: 0.50))
                                            }
                                            .font(.system(size: 11, weight: .medium, design: .rounded))
                                        }
                                        Spacer()
                                        Button(isFull ? "Full" : "Join") {
                                            SoundEffects.shared.play(.buttonPress)
                                            vm.isQuickJoinActive = false
                                            vm.joinLobbyWithRetry(lobbyId: lobby.id)
                                        }
                                        .buttonStyle(PixelButtonStyle(filled: !isFull))
                                        .disabled(isFull)
                                    }
                                    .padding(.vertical, 10)
                                    .padding(.horizontal, 14)
                                    .background(Color.white.opacity(0.05))
                                    .overlay(Rectangle().strokeBorder(Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.20), lineWidth: 1))
                                }
                            }
                        }
                    }
                    .frame(height: 280)
                }

                VStack(spacing: 10) {
                    if !showingCreateForm {
                        HStack(spacing: 10) {
                            Button(action: {
                                SoundEffects.shared.play(.enterArena)
                                vm.quickJoinFirstLobbyWithRetry(waitForLobbySnapshot: true, attemptsRemaining: 6)
                            }) {
                                HStack(spacing: 6) {
                                    Image(systemName: "star.fill")
                                    Text("Quick Join")
                                }
                                .frame(maxWidth: .infinity)
                            }
                            .keyboardShortcut(.defaultAction)
                            .buttonStyle(PixelButtonStyle(filled: true))
                            .controlSize(.large)
                            .disabled(!vm.isConnected)

                            Button(action: {
                                SoundEffects.shared.play(.buttonPress)
                                withAnimation {
                                    showingCreateForm = true
                                    newLobbyName = "\(vm.myPlayer?.name ?? "Player")'s Lobby"
                                }
                                if !vm.hasJoined {
                                    vm.ensureIdentityRegistered(allowFallback: true)
                                }
                            }) {
                                Text("Create")
                            }
                            .buttonStyle(PixelButtonStyle())
                            .controlSize(.large)
                            .disabled(!vm.isConnected)
                        }

                        if vm.isConnected && !vm.hasJoined {
                            Text("Waiting for player registration. Try Quick Join or Create.")
                                .font(.system(size: 9, weight: .medium, design: .rounded))
                                .foregroundStyle(Color(white: 0.38))
                                .frame(maxWidth: .infinity, alignment: .center)
                        }
                    }

                    Button(role: .destructive, action: {
                        SoundEffects.shared.play(.buttonPress)
                        onAction(.quit)
                    }) {
                        Text("Back")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(PixelButtonStyle(danger: true))
                    .controlSize(.large)
                    .padding(.top, showingCreateForm ? 0 : 6)
                }
            }
            .frame(width: 480)
            .padding(.horizontal, 32)
            .padding(.vertical, 32)
            .pixelPanel()
            .shadow(color: Color(red: 0.3, green: 0.6, blue: 1.0).opacity(0.15), radius: 18, x: 0, y: 8)
        }
    }
}

struct LobbyView: View {
    let vm: NinjaGameViewModel
    let onAction: (ExitAction) -> Void
    
    var currentLobby: Lobby? {
        vm.myLobby
    }

    var humanLobbyPlayers: [Player] {
        guard let lobbyId = vm.activeLobbyId else { return [] }
        return vm.players.filter { $0.lobbyId == lobbyId && !$0.name.hasPrefix("Bot ") }
    }

    var lobbyPlayers: [Player] {
        if currentLobby?.isPlaying == true {
            return vm.playersInMyLobby
        }
        return humanLobbyPlayers
    }

    var lobbyPlayerCount: Int {
        lobbyPlayers.count
    }

    var humanPlayerCount: Int {
        humanLobbyPlayers.count
    }

    var readyHumanCount: Int {
        humanLobbyPlayers.filter { $0.isReady }.count
    }

    var botCount: Int {
        max(0, lobbyPlayerCount - humanPlayerCount)
    }

    var openSlots: Int {
        max(0, NinjaGameViewModel.maxPlayersPerLobby - lobbyPlayerCount)
    }

    var lobbyStatusText: String {
        guard let lobby = currentLobby else { return "No active lobby" }
        return lobby.isPlaying ? "Playing" : "Waiting"
    }

    var allReady: Bool {
        !humanLobbyPlayers.isEmpty && humanLobbyPlayers.allSatisfy { $0.isReady }
    }
    
    var myPlayerIsReady: Bool {
        vm.myPlayer?.isReady ?? false
    }

    var body: some View {
        ZStack {
            VStack(spacing: 22) {
                Text("Lobby")
                    .font(.system(size: 26, weight: .heavy, design: .rounded))
                    .foregroundStyle(.white)

                HStack(spacing: 6) {
                    Rectangle()
                        .fill(vm.isConnected ? Color.green : Color.red)
                        .frame(width: 8, height: 8)
                    Text(vm.isConnected ? "CONNECTED" : "DISCONNECTED")
                        .font(.system(size: 11, weight: .medium, design: .rounded))
                        .foregroundStyle(vm.isConnected ? Color(white: 0.60) : .red)
                }

                if !vm.connectionDetail.isEmpty {
                    Text(vm.connectionDetail)
                        .font(.system(size: 10, weight: .medium, design: .rounded))
                        .foregroundStyle(.red)
                }

                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text(currentLobby?.name ?? "Unknown lobby")
                            .font(.system(size: 14, weight: .semibold, design: .rounded))
                            .foregroundStyle(.white)
                        Spacer()
                        Text(lobbyStatusText)
                            .font(.system(size: 11, weight: .medium, design: .rounded))
                            .foregroundStyle(currentLobby?.isPlaying == true ? .orange : .green)
                    }

                    HStack(spacing: 8) {
                        Text("ID #\(currentLobby?.id ?? 0)")
                        Text("·")
                        Text("\(lobbyPlayerCount)/\(NinjaGameViewModel.maxPlayersPerLobby) players")
                        Text("·")
                        Text("\(readyHumanCount)/\(max(1, humanPlayerCount)) ready")
                    }
                    .font(.system(size: 10, weight: .medium, design: .rounded))
                    .foregroundStyle(Color(white: 0.48))

                    HStack {
                        Text("\(openSlots) open slots")
                        if botCount > 0 { Text("· \(botCount) bots") }
                        Spacer()
                    }
                    .font(.system(size: 10, weight: .medium, design: .rounded))
                    .foregroundStyle(Color(white: 0.32))
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
                .background(Color.white.opacity(0.06))
                .overlay(Rectangle().strokeBorder(Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.25), lineWidth: 2))

                // Player list
                VStack(spacing: 6) {
                    HStack {
                        Text("Players")
                            .font(.system(size: 10, weight: .semibold, design: .rounded))
                            .foregroundStyle(Color(white: 0.38))
                        Spacer()
                    }

                    ForEach(lobbyPlayers, id: \.id) { player in
                        HStack {
                            Text((player.id == vm.userId ? "● " : "  ") + player.name)
                                .font(.system(size: 13, weight: player.id == vm.userId ? .semibold : .medium, design: .rounded))
                                .foregroundStyle(player.id == vm.userId ? .white : Color(white: 0.72))
                            Spacer()
                            Text(player.isReady ? "Ready" : "Waiting")
                                .font(.system(size: 11, weight: .medium, design: .rounded))
                                .foregroundStyle(player.isReady ? .green : Color(white: 0.32))
                        }
                        .padding(.vertical, 7)
                        .padding(.horizontal, 10)
                        .background(Color.white.opacity(player.id == vm.userId ? 0.10 : 0.05))
                        .overlay(Rectangle().strokeBorder(
                            player.id == vm.userId
                                ? Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.35)
                                : Color(white: 0.20).opacity(0.35),
                            lineWidth: player.id == vm.userId ? 2 : 1
                        ))
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, 4)

                EventFeedView(
                    events: vm.recentEvents,
                    title: "Recent Events",
                    maxVisible: 5,
                    padded: true
                )
                .frame(maxWidth: .infinity, alignment: .leading)

                VStack(spacing: 8) {
                    Text("Match controls")
                        .font(.system(size: 10, weight: .semibold, design: .rounded))
                        .foregroundStyle(Color(white: 0.35))
                        .frame(maxWidth: .infinity, alignment: .leading)

                    Button(action: {
                        SoundEffects.shared.play(.buttonPress)
                        ToggleReady.invoke()
                    }) {
                        HStack(spacing: 6) {
                            Image(systemName: myPlayerIsReady ? "xmark" : "checkmark")
                            Text(myPlayerIsReady ? "Not Ready" : "Ready Up")
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(PixelButtonStyle(filled: !myPlayerIsReady, danger: myPlayerIsReady))
                    .controlSize(.large)
                    .disabled(!vm.isConnected || !vm.hasJoined)

                    Button(action: {
                        SoundEffects.shared.play(.enterArena)
                        StartMatch.invoke()
                    }) {
                        HStack(spacing: 6) {
                            Image(systemName: "play.fill")
                            Text("Start Match")
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(PixelButtonStyle(filled: true, accentColor: Color(red: 0.15, green: 0.75, blue: 0.30)))
                    .controlSize(.large)
                    .disabled(!vm.isConnected || !vm.hasJoined)

                    Button(role: .destructive, action: {
                        SoundEffects.shared.play(.buttonPress)
                        LeaveLobby.invoke()
                    }) {
                        Text("Leave Lobby")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(PixelButtonStyle(danger: true))
                    .controlSize(.large)
                }
            }
            .frame(width: 400)
            .padding(.horizontal, 24)
            .padding(.vertical, 24)
            .pixelPanel()
            .shadow(color: Color(red: 0.3, green: 0.6, blue: 1.0).opacity(0.12), radius: 16, x: 0, y: 6)
        }
    }
}

// MARK: - Music player (two tracks, crossfade)

@MainActor
@Observable
final class MusicPlayer {
    private enum MusicMode {
        case title
        case game
    }

    private struct FadeState {
        let startTime: TimeInterval
        let duration: TimeInterval
        let fromTitleLevel: Float
        let toTitleLevel: Float
        let fromGameLevel: Float
        let toGameLevel: Float
    }

    var isMuted: Bool = false {
        didSet {
            applyEffectiveVolumes()
        }
    }

    private let titleNominalVolume: Float = 0.55
    private let gameNominalVolume: Float = 0.65
    private let fadeTickInterval: TimeInterval = 1.0 / 60.0

    private var titlePlayer: AVAudioPlayer?
    private var gamePlayer: AVAudioPlayer?
    private var mode: MusicMode = .title

    // Logical (unmuted) levels.
    private var titleLevel: Float = 0
    private var gameLevel: Float = 0
    private var fadeState: FadeState?
    private var fadeTimer: Timer?
    private var isInterrupted = false

#if canImport(UIKit)
    private var interruptionObserver: NSObjectProtocol?
    private var routeChangeObserver: NSObjectProtocol?
    private var mediaResetObserver: NSObjectProtocol?
#endif

    init() {
        titlePlayer = makePlayer(resource: "SpaceTimeDB Survivors", exts: ["m4a", "wav"])
        gamePlayer = makePlayer(resource: "SpaceTimeDB Survivors - Alternate Music", exts: ["m4a", "wav"])
        applyEffectiveVolumes()
        installInterruptionObserversIfSupported()
    }

    func playTitle() {
        transition(to: .title, duration: 2.5, force: true)
    }

    func crossfadeToGame() {
        transition(to: .game, duration: 1.5)
    }

    func switchToTitleMusic() {
        transition(to: .title, duration: 1.0)
    }

    func toggleMute() {
        isMuted.toggle()
    }

    private func transition(to newMode: MusicMode, duration: TimeInterval, force: Bool = false) {
        if !force && mode == newMode && fadeState == nil {
            resumePlayersForCurrentLevels()
            return
        }
        mode = newMode
        if isInterrupted {
            // Defer playback while interrupted; keep logical targets consistent.
            titleLevel = (newMode == .title) ? titleNominalVolume : 0
            gameLevel  = (newMode == .game) ? gameNominalVolume  : 0
            fadeState = nil
            fadeTimer?.invalidate()
            fadeTimer = nil
            applyEffectiveVolumes()
            return
        }
        // During a crossfade both tracks must be playing before we adjust volumes.
        // Ensure the incoming track starts (at its current level) before fading.
        ensurePlayerLoaded(for: .title)
        ensurePlayerLoaded(for: .game)
        startPlaybackIfNeeded(titlePlayer)
        startPlaybackIfNeeded(gamePlayer)
        let targetTitle = (newMode == .title) ? titleNominalVolume : 0
        let targetGame  = (newMode == .game)  ? gameNominalVolume  : 0
        startFade(toTitleLevel: targetTitle, toGameLevel: targetGame, duration: duration)
    }

    private func ensurePlayerLoaded(for mode: MusicMode) {
        switch mode {
        case .title:
            if titlePlayer == nil {
                titlePlayer = makePlayer(resource: "SpaceTimeDB Survivors", exts: ["m4a", "wav"])
                applyEffectiveVolumes()
            }
        case .game:
            if gamePlayer == nil {
                gamePlayer = makePlayer(resource: "SpaceTimeDB Survivors - Alternate Music", exts: ["m4a", "wav"])
                applyEffectiveVolumes()
            }
        }
    }

    private func startPlaybackIfNeeded(_ player: AVAudioPlayer?) {
        guard let player else { return }
        guard !isInterrupted else { return }
        if !player.isPlaying {
            player.prepareToPlay()
            if !player.play() {
                print("[MusicPlayer] Failed to start playback for \(player.url?.lastPathComponent ?? "unknown")")
            }
        }
    }

    private func resumePlayersForCurrentLevels() {
        ensurePlayerLoaded(for: .title)
        ensurePlayerLoaded(for: .game)
        if titleLevel > 0.001 {
            startPlaybackIfNeeded(titlePlayer)
        } else {
            titlePlayer?.pause()
        }
        if gameLevel > 0.001 {
            startPlaybackIfNeeded(gamePlayer)
        } else {
            gamePlayer?.pause()
        }
        applyEffectiveVolumes()
    }

    private func startFade(toTitleLevel: Float, toGameLevel: Float, duration: TimeInterval) {
        fadeTimer?.invalidate()
        fadeTimer = nil

        let clampedDuration = max(0, duration)
        if clampedDuration == 0 {
            titleLevel = toTitleLevel
            gameLevel = toGameLevel
            applyEffectiveVolumes()
            pauseSilentPlayers(titleTarget: toTitleLevel, gameTarget: toGameLevel)
            return
        }

        fadeState = FadeState(
            startTime: Date.timeIntervalSinceReferenceDate,
            duration: clampedDuration,
            fromTitleLevel: titleLevel,
            toTitleLevel: toTitleLevel,
            fromGameLevel: gameLevel,
            toGameLevel: toGameLevel
        )

        let timer = Timer(timeInterval: fadeTickInterval, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.tickFade()
            }
        }
        fadeTimer = timer
        RunLoop.main.add(timer, forMode: .common)
    }

    private func tickFade() {
        guard let fade = fadeState else { return }
        let now = Date.timeIntervalSinceReferenceDate
        let t = Float(max(0, min(1, (now - fade.startTime) / fade.duration)))
        titleLevel = fade.fromTitleLevel + (fade.toTitleLevel - fade.fromTitleLevel) * t
        gameLevel = fade.fromGameLevel + (fade.toGameLevel - fade.fromGameLevel) * t
        applyEffectiveVolumes()

        if t >= 1 {
            let finishedTitleTarget = fade.toTitleLevel
            let finishedGameTarget  = fade.toGameLevel
            fadeState = nil
            fadeTimer?.invalidate()
            fadeTimer = nil
            pauseSilentPlayers(titleTarget: finishedTitleTarget, gameTarget: finishedGameTarget)
        }
    }

    /// Pauses whichever player faded down to silence.
    /// Taking the targets as parameters avoids reading `fadeState` after it is cleared.
    private func pauseSilentPlayers(titleTarget: Float, gameTarget: Float) {
        if titleTarget <= 0.001 { titlePlayer?.pause() }
        if gameTarget  <= 0.001 { gamePlayer?.pause()  }
    }

    private func applyEffectiveVolumes() {
        let titleVolume = isMuted ? 0 : titleLevel
        let gameVolume = isMuted ? 0 : gameLevel
        titlePlayer?.volume = titleVolume
        gamePlayer?.volume = gameVolume
    }

    private func makePlayer(resource: String, exts: [String]) -> AVAudioPlayer? {
        for ext in exts {
            guard let url = Bundle.module.url(forResource: resource, withExtension: ext) else { continue }
            let player = try? AVAudioPlayer(contentsOf: url)
            player?.numberOfLoops = -1
            player?.prepareToPlay()
            if player != nil {
                return player
            }
        }
        print("[MusicPlayer] Missing or unreadable resource: \(resource)")
        return nil
    }

#if canImport(UIKit)
    // MARK: - iOS audio session interruption (calls/Siri/alarms)
    private func installInterruptionObserversIfSupported() {
        interruptionObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.interruptionNotification,
            object: AVAudioSession.sharedInstance(),
            queue: .main
        ) { [weak self] notification in
            Task { @MainActor [weak self] in
                self?.handleAudioInterruption(notification)
            }
        }
        routeChangeObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.routeChangeNotification,
            object: AVAudioSession.sharedInstance(),
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self, !self.isInterrupted else { return }
                self.resumePlayersForCurrentLevels()
            }
        }
        mediaResetObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.mediaServicesWereResetNotification,
            object: AVAudioSession.sharedInstance(),
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                self.titlePlayer = self.makePlayer(resource: "SpaceTimeDB Survivors", exts: ["m4a", "wav"])
                self.gamePlayer = self.makePlayer(resource: "SpaceTimeDB Survivors - Alternate Music", exts: ["m4a", "wav"])
                self.resumePlayersForCurrentLevels()
            }
        }
    }

    private func handleAudioInterruption(_ notification: Notification) {
        guard let info = notification.userInfo,
              let rawType = info[AVAudioSessionInterruptionTypeKey] as? UInt,
              let type = AVAudioSession.InterruptionType(rawValue: rawType) else {
            return
        }

        switch type {
        case .began:
            isInterrupted = true
            // Snap any active fade to its target so state remains deterministic.
            if let fade = fadeState {
                titleLevel = fade.toTitleLevel
                gameLevel = fade.toGameLevel
                fadeState = nil
                fadeTimer?.invalidate()
                fadeTimer = nil
            }
            titlePlayer?.pause()
            gamePlayer?.pause()
        case .ended:
            let shouldResume = (info[AVAudioSessionInterruptionOptionKey] as? UInt)
                .map { AVAudioSession.InterruptionOptions(rawValue: $0).contains(.shouldResume) } ?? false
            isInterrupted = false
            if shouldResume {
                resumePlayersForCurrentLevels()
            }
        @unknown default:
            break
        }
    }
#else
    // MARK: - macOS audio route / hardware change recovery
    // macOS has no AVAudioSession, but hardware route changes (e.g. headphones
    // plug/unplug, Bluetooth handoff, display sleep) can silently reset the
    // underlying audio unit and cause players to stop. We recover by observing
    // AVAudioPlayer's notification and restarting playback if needed.
    private var routeChangeObserver: NSObjectProtocol?

    private func installInterruptionObserversIfSupported() {
        // AVAudioPlayer doesn't stop on macOS route changes, but we watch for
        // app-level background/foreground transitions that can drop the audio
        // device on macOS (e.g. display sleep on Apple Silicon).
        routeChangeObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                // If the player silently stopped while the app was inactive, restart.
                self.isInterrupted = false
                self.resumePlayersForCurrentLevels()
            }
        }
    }
#endif
}

// MARK: - Sound Effects

/// Synthesizes all UI sound effects using AVAudioEngine.
///
/// Design goals / robustness:
/// - All PCM buffers are pre-synthesized on a background thread at init, so
///   `play()` only schedules a pre-built buffer — zero main-thread synthesis.
/// - A fixed pool of `AVAudioPlayerNode`s per sound handles polyphonic rapid
///   repeats (e.g. picking up several weapons in a row) without any node leak.
/// - Handles `AVAudioEngineConfigurationChangeNotification` (headphones
///   plug/unplug, BT route change, sleep/wake on macOS) by restarting the
///   engine and re-attaching all nodes automatically.
/// - A silent warm-up buffer is played at start so the audio graph is fully
///   active before the first real sound fires.
@MainActor
final class SoundEffects {
    static let shared = SoundEffects()

    enum Sound: CaseIterable {
        case buttonPress    // soft 2-note chime C5→E5
        case menuButton     // slightly lower chime B4→D5
        case enterArena     // rising major arpeggio C5 E5 G5
        case menuOpen       // descending minor 2nd E5→Eb5
        case menuClose      // ascending perfect 4th C5→F5
        case respawn        // bright 4-note fanfare C5 E5 G5 C6
        case weaponPickup   // metallic ting (high sine, fast decay)
        case attack         // percussive thwack (low sawtooth)
        case death          // dramatic descending tritone swell
        case muteToggle     // single muffled pop
    }

    var isMuted = false {
        didSet {
            if !isMuted {
                flushPendingSounds()
            }
        }
    }

    // MARK: - Private

    private let engine = AVAudioEngine()
    private var mixer: AVAudioMixerNode { engine.mainMixerNode }
    /// Permanent node used to keep a valid graph before any SFX pool exists.
    private let bootstrapNode = AVAudioPlayerNode()

    /// Each sound gets a small round-robin pool of player nodes so rapid
    /// repeats of the same sound overlap cleanly without node thrash.
    private var pools: [Sound: NodePool] = [:]

    /// Pre-built PCM buffers, keyed by sound. Set from background thread,
    /// then only read on main actor, so access is safe after init completes.
    private var buffers: [Sound: AVAudioPCMBuffer] = [:]
    private var buffersReady = false
    private var pendingSounds: [Sound] = []
    private let maxPendingSounds = 24
    private var isEngineInterrupted = false
    private var lastPlayedAt: [Sound: TimeInterval] = [:]
    private var burstWindow: [Sound: (start: TimeInterval, count: Int)] = [:]
    private var globalBurstWindow: (start: TimeInterval, count: Int) = (start: 0, count: 0)

    private var configChangeObserver: NSObjectProtocol?
#if canImport(UIKit)
    private var interruptionObserver: NSObjectProtocol?
    private var routeChangeObserver: NSObjectProtocol?
    private var mediaResetObserver: NSObjectProtocol?
#else
    private var appActiveObserver: NSObjectProtocol?
#endif

    private init() {
        ensureBootstrapAttached()
        // Start the engine immediately so the output node format is available.
        restartEngine()
        // Synthesize buffers + warm up on a background thread.
        buildBuffersAndWarmUp()
        // Receive route-change / config-reset notifications.
        configChangeObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange,
            object: engine,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in self?.handleEngineReset() }
        }
        installInterruptionObserversIfSupported()
    }

    // MARK: - Public API

    func play(_ sound: Sound) {
        guard !isMuted else { return }
        let now = monotonicNow()
        guard !shouldDrop(sound: sound, now: now) else { return }
        guard consumeGlobalBudget(now: now) else { return }
        guard buffersReady, !isEngineInterrupted else {
            enqueuePending(sound)
            return
        }
        playNow(sound)
    }

    // MARK: - Engine lifecycle

    private func restartEngine() {
        ensureBootstrapAttached()
        if engine.isRunning { engine.stop() }
        do {
            // With bootstrap node attached, start is safe on macOS and iOS.
            try engine.start()
        } catch {
            print("[SoundEffects] AVAudioEngine start failed: \(error)")
        }
    }

    private func handleEngineReset() {
        recoverEngine()
        flushPendingSounds()
    }

    // MARK: - Pool management

    private func pool(for sound: Sound) -> NodePool {
        if let existing = pools[sound] { return existing }
        let p = NodePool(size: poolSize(for: sound), engine: engine, mixer: mixer)
        pools[sound] = p
        return p
    }

    private func poolSize(for sound: Sound) -> Int {
        switch sound {
        case .weaponPickup: return 2
        case .attack:       return 2
        default:            return 2
        }
    }

    // MARK: - Buffer synthesis (background)

    private func buildBuffersAndWarmUp() {
        // Capture definitions as sendable value types for the background task.
        let definitions = SoundEffects.soundDefinitions()
        Task.detached(priority: .userInitiated) {
            var built: [Sound: AVAudioPCMBuffer] = [:]
            for (sound, def) in definitions {
                if let buf = Self.synthesize(def) { built[sound] = buf }
            }
            // Deliver results and warm up back on main actor.
            await MainActor.run {
                self.buffers = built
                self.buffersReady = true
                self.warmUpEngine(sampleRate: 44100)
                self.flushPendingSounds()
            }
        }
    }

    /// Play a single frame of silence to prime the audio graph.
    private func warmUpEngine(sampleRate: Double) {
        guard ensureEngineRunning() else { return }
        guard let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 1),
              let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 1) else { return }
        buf.frameLength = 1
        buf.floatChannelData![0][0] = 0
        let primer = AVAudioPlayerNode()
        engine.attach(primer)
        engine.connect(primer, to: mixer, format: format)
        primer.play()
        primer.scheduleBuffer(buf, completionCallbackType: .dataRendered) { [weak self] _ in
            Task { @MainActor [weak self] in self?.engine.detach(primer) }
        }
    }

    private func playNow(_ sound: Sound) {
        guard let buffer = buffers[sound] else {
            enqueuePending(sound)
            return
        }
        guard ensureEngineRunning() else {
            enqueuePending(sound)
            return
        }
        let played = pool(for: sound).play(buffer: buffer, through: engine)
        if !played {
            enqueuePending(sound)
        }
    }

    private func enqueuePending(_ sound: Sound) {
        if isBurstProne(sound), pendingSounds.last == sound {
            return
        }
        if pendingSounds.count >= maxPendingSounds {
            pendingSounds.removeFirst(pendingSounds.count - maxPendingSounds + 1)
        }
        pendingSounds.append(sound)
    }

    private func flushPendingSounds() {
        guard !isMuted, buffersReady, !isEngineInterrupted else { return }
        guard !pendingSounds.isEmpty else { return }
        let queued = pendingSounds
        pendingSounds.removeAll(keepingCapacity: true)
        let now = monotonicNow()
        for sound in queued {
            if shouldDrop(sound: sound, now: now) {
                continue
            }
            if !consumeGlobalBudget(now: now) {
                break
            }
            playNow(sound)
        }
    }

    private func monotonicNow() -> TimeInterval {
        ProcessInfo.processInfo.systemUptime
    }

    private func isBurstProne(_ sound: Sound) -> Bool {
        switch sound {
        case .attack, .weaponPickup:
            return true
        default:
            return false
        }
    }

    private func limits(for sound: Sound) -> (minGap: TimeInterval, maxPerWindow: Int) {
        switch sound {
        case .attack:
            // Combat can emit very dense hits; keep this conservative to avoid
            // audio I/O overload while preserving responsiveness.
            return (0.12, 2)
        case .weaponPickup:
            return (0.10, 2)
        case .menuOpen, .menuClose:
            return (0.08, 2)
        default:
            return (0.0, 8)
        }
    }

    private func shouldDrop(sound: Sound, now: TimeInterval) -> Bool {
        let rule = limits(for: sound)
        if rule.minGap > 0, let last = lastPlayedAt[sound], (now - last) < rule.minGap {
            return true
        }

        let windowDuration: TimeInterval = 0.10
        var state = burstWindow[sound] ?? (start: now, count: 0)
        if (now - state.start) > windowDuration {
            state = (start: now, count: 0)
        }
        if state.count >= rule.maxPerWindow {
            burstWindow[sound] = state
            return true
        }
        state.count += 1
        burstWindow[sound] = state
        lastPlayedAt[sound] = now
        return false
    }

    private func consumeGlobalBudget(now: TimeInterval) -> Bool {
        let windowDuration: TimeInterval = 0.10
        let maxPerWindow = 5
        if (now - globalBurstWindow.start) > windowDuration {
            globalBurstWindow = (start: now, count: 0)
        }
        guard globalBurstWindow.count < maxPerWindow else { return false }
        globalBurstWindow.count += 1
        return true
    }

    private func ensureEngineRunning() -> Bool {
        if engine.isRunning {
            return true
        }
        recoverEngine()
        return engine.isRunning
    }

    private func recoverEngine() {
        ensureBootstrapAttached()
        for pool in pools.values {
            pool.reattach(to: engine, mixer: mixer)
        }
        restartEngine()
    }

    private func ensureBootstrapAttached() {
        if !engine.attachedNodes.contains(bootstrapNode) {
            engine.attach(bootstrapNode)
            if let format = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 1) {
                engine.connect(bootstrapNode, to: mixer, format: format)
            }
        }
    }

    // MARK: - Sound definitions

    private enum Waveform: Sendable { case sine, triangle, sawtooth }

    private struct NoteSpec: Sendable {
        let freq: Double; let start: Double; let dur: Double
    }
    private struct SoundDef: Sendable {
        let notes: [NoteSpec]; let wave: Waveform; let gain: Float
    }

    // Waveform is not Sendable by default since it's an enum in a non-Sendable context.
    // Explicitly mark it for Task.detached.
    private static func soundDefinitions() -> [(Sound, SoundDef)] {
        [
            (.buttonPress,   .init(notes: [.init(freq: 523.25, start: 0.00, dur: 0.06),
                                           .init(freq: 659.25, start: 0.06, dur: 0.06)],
                                   wave: .sine, gain: 0.18)),
            (.menuButton,    .init(notes: [.init(freq: 493.88, start: 0.00, dur: 0.05),
                                           .init(freq: 587.33, start: 0.05, dur: 0.06)],
                                   wave: .sine, gain: 0.14)),
            (.enterArena,    .init(notes: [.init(freq: 523.25, start: 0.00, dur: 0.07),
                                           .init(freq: 659.25, start: 0.07, dur: 0.07),
                                           .init(freq: 783.99, start: 0.14, dur: 0.10)],
                                   wave: .sine, gain: 0.20)),
            (.menuOpen,      .init(notes: [.init(freq: 659.25, start: 0.00, dur: 0.05),
                                           .init(freq: 622.25, start: 0.05, dur: 0.08)],
                                   wave: .triangle, gain: 0.15)),
            (.menuClose,     .init(notes: [.init(freq: 523.25, start: 0.00, dur: 0.05),
                                           .init(freq: 698.46, start: 0.05, dur: 0.08)],
                                   wave: .triangle, gain: 0.15)),
            (.respawn,       .init(notes: [.init(freq: 523.25, start: 0.00, dur: 0.07),
                                           .init(freq: 659.25, start: 0.07, dur: 0.07),
                                           .init(freq: 783.99, start: 0.14, dur: 0.07),
                                           .init(freq: 1046.5, start: 0.21, dur: 0.14)],
                                   wave: .sine, gain: 0.22)),
            (.weaponPickup,  .init(notes: [.init(freq: 1174.66, start: 0.00, dur: 0.04),
                                           .init(freq: 1396.91, start: 0.03, dur: 0.05)],
                                   wave: .sine, gain: 0.25)),
            (.attack,        .init(notes: [.init(freq: 180.0,  start: 0.00, dur: 0.03),
                                           .init(freq: 120.0,  start: 0.03, dur: 0.04)],
                                   wave: .sawtooth, gain: 0.28)),
            (.death,         .init(notes: [.init(freq: 440.0,  start: 0.00, dur: 0.12),
                                           .init(freq: 311.13, start: 0.10, dur: 0.14),
                                           .init(freq: 220.0,  start: 0.22, dur: 0.20)],
                                   wave: .triangle, gain: 0.30)),
            (.muteToggle,    .init(notes: [.init(freq: 300.0,  start: 0.00, dur: 0.04)],
                                   wave: .sine, gain: 0.12)),
        ]
    }

    // MARK: - PCM synthesis (pure function, runs on background thread)

    private nonisolated static func synthesize(_ def: SoundDef) -> AVAudioPCMBuffer? {
        let sampleRate: Double = 44100
        let totalDuration = def.notes.map { $0.start + $0.dur + 0.02 }.max() ?? 0.1
        let frameCount = AVAudioFrameCount(totalDuration * sampleRate)
        guard let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 1),
              let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frameCount) else { return nil }
        buffer.frameLength = frameCount
        let data = buffer.floatChannelData![0]
        for i in 0..<Int(frameCount) { data[i] = 0 }

        for note in def.notes {
            let startFrame  = Int(note.start * sampleRate)
            let noteFrames  = Int(note.dur   * sampleRate)
            let attackFrames  = max(1, Int(min(0.008, note.dur * 0.10) * sampleRate))
            let releaseFrames = max(1, Int(min(0.020, note.dur * 0.25) * sampleRate))

            for i in 0..<noteFrames {
                let gf = startFrame + i
                guard gf < Int(frameCount) else { break }
                let phase = 2.0 * Double.pi * note.freq * Double(i) / sampleRate
                let raw: Double
                switch def.wave {
                case .sine:     raw = sin(phase)
                case .triangle: raw = 2.0 / Double.pi * asin(sin(phase))
                case .sawtooth: raw = 2.0 * (phase / (2 * .pi) - floor(phase / (2 * .pi) + 0.5))
                }
                let env: Double
                if i < attackFrames {
                    env = Double(i) / Double(attackFrames)
                } else if i >= noteFrames - releaseFrames {
                    env = Double(noteFrames - i) / Double(releaseFrames)
                } else {
                    env = 1.0
                }
                data[gf] += Float(raw * env * Double(def.gain))
            }
        }
        return buffer
    }

#if canImport(UIKit)
    private func installInterruptionObserversIfSupported() {
        interruptionObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.interruptionNotification,
            object: AVAudioSession.sharedInstance(),
            queue: .main
        ) { [weak self] note in
            Task { @MainActor [weak self] in
                self?.handleInterruption(note)
            }
        }
        routeChangeObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.routeChangeNotification,
            object: AVAudioSession.sharedInstance(),
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self, !self.isEngineInterrupted else { return }
                self.recoverEngine()
                self.flushPendingSounds()
            }
        }
        mediaResetObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.mediaServicesWereResetNotification,
            object: AVAudioSession.sharedInstance(),
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                self.isEngineInterrupted = false
                self.recoverEngine()
                self.flushPendingSounds()
            }
        }
    }

    private func handleInterruption(_ notification: Notification) {
        guard let info = notification.userInfo,
              let rawType = info[AVAudioSessionInterruptionTypeKey] as? UInt,
              let type = AVAudioSession.InterruptionType(rawValue: rawType) else {
            return
        }

        switch type {
        case .began:
            isEngineInterrupted = true
            if engine.isRunning {
                engine.pause()
            }
        case .ended:
            let shouldResume = (info[AVAudioSessionInterruptionOptionKey] as? UInt)
                .map { AVAudioSession.InterruptionOptions(rawValue: $0).contains(.shouldResume) } ?? false
            isEngineInterrupted = false
            if shouldResume {
                recoverEngine()
                flushPendingSounds()
            }
        @unknown default:
            break
        }
    }
#else
    private func installInterruptionObserversIfSupported() {
        appActiveObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.recoverEngine()
                self?.flushPendingSounds()
            }
        }
    }
#endif
}

// MARK: - NodePool

/// A fixed-size round-robin pool of `AVAudioPlayerNode`s for one sound type.
/// Prevents node exhaustion under rapid repeated triggers.
@MainActor
private final class NodePool {
    private var nodes: [AVAudioPlayerNode] = []
    private var cursor = 0
    private weak var engine: AVAudioEngine?
    private weak var mixer: AVAudioMixerNode?

    init(size: Int, engine: AVAudioEngine, mixer: AVAudioMixerNode) {
        self.engine = engine
        self.mixer  = mixer
        nodes = (0..<size).map { _ in Self.makeNode(engine: engine, mixer: mixer) }
    }

    func play(buffer: AVAudioPCMBuffer, through engine: AVAudioEngine) -> Bool {
        guard engine.isRunning else {
            print("[NodePool] Engine not running — skipping sound")
            return false
        }
        let node = nodes[cursor % nodes.count]
        cursor += 1
        // Stop any currently-playing sound on this node slot (round-robin eviction).
        if node.isPlaying { node.stop() }
        node.scheduleBuffer(buffer, at: nil, options: .interrupts)
        node.play()
        return true
    }

    /// Called after an engine configuration reset to re-attach nodes.
    func reattach(to engine: AVAudioEngine, mixer: AVAudioMixerNode) {
        self.engine = engine
        self.mixer  = mixer
        for node in nodes {
            if !engine.attachedNodes.contains(node) {
                Self.attach(node: node, engine: engine, mixer: mixer)
            }
        }
    }

    private static func makeNode(engine: AVAudioEngine, mixer: AVAudioMixerNode) -> AVAudioPlayerNode {
        let node = AVAudioPlayerNode()
        attach(node: node, engine: engine, mixer: mixer)
        return node
    }

    private static func attach(node: AVAudioPlayerNode, engine: AVAudioEngine, mixer: AVAudioMixerNode) {
        engine.attach(node)
        // Use a fixed low-overhead mono format matching our synthesis rate.
        if let format = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 1) {
            engine.connect(node, to: mixer, format: format)
        }
    }
}

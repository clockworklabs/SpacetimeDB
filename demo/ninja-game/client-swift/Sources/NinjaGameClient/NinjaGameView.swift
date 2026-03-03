import SwiftUI
import Observation
import SpacetimeDB
#if canImport(AppKit)
import AppKit
#endif

let worldMin: Float = 0
let worldMax: Float = 1000
let playerEdgePadding: Float = 18
let weaponSpawnPadding: Float = 24
let weaponSpawnInterval: TimeInterval = 2.8
let maxGroundWeapons = 20

public enum ExitAction {
    case resetName
    case quit
}

public enum SpacetimeEnvironment: String, CaseIterable, Identifiable {
    case local = "Local Server"
    case prod = "Prod DB"

    public var id: String { self.rawValue }

    public var url: URL {
        switch self {
        case .local:
            return URL(string: "http://127.0.0.1:3000")!
        case .prod:
            return URL(string: "wss://maincloud.spacetimedb.com")!
        }
    }
}

public struct NinjaGameView: View {
    @State private var vm: NinjaGameViewModel
    @State private var showingResetNameDialog = false
    @State private var resetNameDraft = ""
    private let ownsViewModel: Bool
    let onExit: ((ExitAction) -> Void)?
    var onMusicChange: ((Bool) -> Void)? // true = game music, false = title music
    var onMuteToggle: (() -> Void)?
    var isMuted: Bool
    var isBackground: Bool

    /// Pass a name to auto-join immediately on appear.
    public init(
        isBackground: Bool = false, 
        initialName: String? = nil, 
        isMuted: Bool = false, 
        injectedVM: NinjaGameViewModel? = nil,
        onMuteToggle: (() -> Void)? = nil, 
        onExit: ((ExitAction) -> Void)? = nil, 
        onMusicChange: ((Bool) -> Void)? = nil
    ) {
        if let injected = injectedVM {
            _vm = State(initialValue: injected)
            self.ownsViewModel = false
        } else {
            _vm = State(initialValue: NinjaGameViewModel(initialName: initialName))
            self.ownsViewModel = true
        }
        self.isBackground = isBackground
        self.isMuted = isMuted
        self.onMuteToggle = onMuteToggle
        self.onExit = onExit
        self.onMusicChange = onMusicChange
    }

    private var isActivePlayState: Bool {
        vm.hasJoined && !vm.isMenuOpen && !vm.isDead
    }

    public var body: some View {
        ZStack {
            // Game Area — camera follows local player
            GeometryReader { _ in
                ZStack {
                    SwiftUIGameViewport(vm: vm)
                        .background(
                            LinearGradient(
                                colors: [SurvivorsTheme.backdropBottom, SurvivorsTheme.backdropTop],
                                startPoint: .top,
                                endPoint: .bottom
                            )
                        )
                        .clipped()

                    #if !os(macOS)
                    if let base = vm.jsBase {
                        ZStack {
                            Circle()
                                .strokeBorder(Color.primary.opacity(0.28), lineWidth: 1)
                                .frame(width: 100, height: 100)
                            Circle()
                                .strokeBorder(Color.primary.opacity(0.65), lineWidth: 2)
                                .frame(width: 50, height: 50)
                                .offset(x: vm.jsVector.dx, y: vm.jsVector.dy)
                        }
                        .position(base)
                    }
                    #endif
                }

                // HUD Layer Overlay
                VStack(spacing: 0) {
                    if !isBackground {
                        statusBar
                            .padding(.top, 12)
                            .padding(.horizontal, 16)
                    }

                    HStack {
                        if !isBackground {
                            EventFeedView(events: vm.recentEvents)
                                .padding(.top, 12)
                                .padding(.leading, 12)
                        }
                        Spacer()
                    }

                    Spacer()

                    if !isBackground {
                        playingFooter
                            .padding(.horizontal, 16)
                            .padding(.bottom, 12)
                    }
                }
            }
            #if !os(macOS)
            .gesture(
                DragGesture(minimumDistance: 5)
                    .onChanged { val in
                        vm.updateJoystick(active: true, base: val.startLocation, current: val.location)
                    }
                    .onEnded { _ in
                        vm.updateJoystick(active: false)
                    }
            )
            #endif
        }
        .grayscale(vm.isDead ? 1.0 : 0.0) // B&W effect when dead
        .overlay {
            if !isBackground && vm.isDead {
                ZStack {
                    Color.black.opacity(0.35)
                        .ignoresSafeArea()

                    VStack(spacing: 16) {
                        Text("Eliminated")
                            .font(.system(size: 18, weight: .heavy, design: .rounded))
                            .foregroundStyle(Color(red: 1.0, green: 0.35, blue: 0.35))

                        Text("Wait for an opening, then rejoin.")
                            .font(.system(size: 11, weight: .medium, design: .rounded))
                            .foregroundStyle(Color(white: 0.45))
                            .multilineTextAlignment(.center)

                        Button(action: {
                            Respawn.invoke()
                            vm.isMenuOpen = false
                            SoundEffects.shared.play(.respawn)
                        }) {
                            HStack(spacing: 6) {
                                Image(systemName: "arrow.clockwise")
                                Text("Respawn")
                            }
                            .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(PixelButtonStyle(filled: true))
                        .controlSize(.large)
                    }
                    .frame(width: 340)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 18)
                    .pixelPanel()
                    .shadow(color: Color(red: 0.3, green: 0.6, blue: 1.0).opacity(0.20), radius: 24, x: 0, y: 8)
                }
                .transition(.opacity)
                .ignoresSafeArea()
            }
        }
        .overlay {
            if !isBackground && vm.isMenuOpen {
                ZStack {
                    Color.black.opacity(0.45)
                        .ignoresSafeArea()
                        .onTapGesture {
                            SoundEffects.shared.play(.menuClose)
                            showingResetNameDialog = false
                            vm.isMenuOpen = false
                        }

                    VStack(spacing: 12) {
                        HStack(alignment: .top) {
                            VStack(alignment: .leading, spacing: 6) {
                                Text("Paused")
                                    .font(.system(size: 20, weight: .heavy, design: .rounded))
                                    .foregroundStyle(.white)
                                Text(vm.myPlayer?.name ?? "Connected Player")
                                    .font(.system(size: 11, weight: .medium, design: .rounded))
                                    .foregroundStyle(Color(white: 0.52))
                                Text("Choose your next move.")
                                    .font(.system(size: 10, weight: .medium, design: .rounded))
                                    .foregroundStyle(Color(white: 0.40))
                            }
                            Spacer(minLength: 12)
                            Text("ESC • RESUME")
                                .font(.system(size: 10, weight: .heavy, design: .rounded))
                                .foregroundStyle(Color(white: 0.42))
                                .padding(.horizontal, 9)
                                .padding(.vertical, 6)
                                .background(Color.white.opacity(0.06))
                                .overlay(Rectangle().strokeBorder(Color(white: 0.26), lineWidth: 1))
                        }

                        ViewThatFits(in: .horizontal) {
                            HStack(spacing: 8) {
                                if let me = vm.myPlayer {
                                    HudHealthMeter(health: me.health)
                                        .frame(width: 148)
                                    HudStatChip(label: "Kills", value: "\(me.kills)", tint: .orange)
                                    HudStatChip(label: "Swords", value: "\(me.weaponCount)", tint: SurvivorsTheme.accent)
                                }
                                HudStatChip(label: "Players", value: "\(vm.players.count)", tint: .green)
                            }

                            VStack(alignment: .leading, spacing: 8) {
                                if let me = vm.myPlayer {
                                    HudHealthMeter(health: me.health)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                    HStack(spacing: 8) {
                                        HudStatChip(label: "Kills", value: "\(me.kills)", tint: .orange)
                                        HudStatChip(label: "Swords", value: "\(me.weaponCount)", tint: SurvivorsTheme.accent)
                                        HudStatChip(label: "Players", value: "\(vm.players.count)", tint: .green)
                                    }
                                } else {
                                    HudStatChip(label: "Players", value: "\(vm.players.count)", tint: .green)
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)

                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text("SESSION")
                                    .font(.system(size: 10, weight: .heavy, design: .rounded))
                                    .foregroundStyle(Color(white: 0.44))
                                Spacer()
                            }

                            if let lobby = vm.myLobby {
                                let count = vm.playerCount(forLobbyId: lobby.id)
                                let maxCount = NinjaGameViewModel.maxPlayersPerLobby
                                HStack(alignment: .firstTextBaseline) {
                                    Text(lobby.name)
                                        .font(.system(size: 14, weight: .heavy, design: .rounded))
                                        .foregroundStyle(.white)
                                        .lineLimit(1)
                                    Spacer(minLength: 8)
                                    Text("ID #\(lobby.id)")
                                        .font(.system(size: 10, weight: .bold, design: .rounded))
                                        .foregroundStyle(Color(white: 0.42))
                                }

                                HStack(spacing: 8) {
                                    Text("\(count)/\(maxCount) players")
                                    Text("·")
                                    Text(lobby.isPlaying ? "Playing" : "Waiting")
                                }
                                .font(.system(size: 10, weight: .bold, design: .rounded))
                                .foregroundStyle(count >= maxCount ? .red : Color(white: 0.50))
                            } else {
                                Text("NO ACTIVE LOBBY")
                                    .font(.system(size: 11, weight: .bold, design: .rounded))
                                    .foregroundStyle(Color(white: 0.40))
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(10)
                        .background(Color.white.opacity(0.06))
                        .overlay(Rectangle().strokeBorder(Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.26), lineWidth: 2))

                        ViewThatFits(in: .horizontal) {
                            HStack(spacing: 8) {
                                Button {
                                    SoundEffects.shared.play(.menuClose)
                                    showingResetNameDialog = false
                                    vm.isMenuOpen = false
                                } label: {
                                    Label("CONTINUE", systemImage: "play.fill")
                                        .frame(maxWidth: .infinity)
                                }
                                .buttonStyle(PixelButtonStyle(filled: true))
                                .keyboardShortcut(.defaultAction)

                                Button {
                                    SoundEffects.shared.play(.menuButton)
                                    resetNameDraft = vm.myPlayer?.name ?? vm.initialName ?? ""
                                    showingResetNameDialog = true
                                } label: {
                                    Label("EDIT NAME", systemImage: "person.text.rectangle")
                                        .frame(maxWidth: .infinity)
                                }
                                .buttonStyle(PixelButtonStyle())
                            }
                            .controlSize(.large)

                            VStack(spacing: 8) {
                                Button {
                                    SoundEffects.shared.play(.menuClose)
                                    showingResetNameDialog = false
                                    vm.isMenuOpen = false
                                } label: {
                                    Label("CONTINUE", systemImage: "play.fill")
                                        .frame(maxWidth: .infinity)
                                }
                                .buttonStyle(PixelButtonStyle(filled: true))
                                .controlSize(.large)
                                .keyboardShortcut(.defaultAction)

                                Button {
                                    SoundEffects.shared.play(.menuButton)
                                    resetNameDraft = vm.myPlayer?.name ?? vm.initialName ?? ""
                                    showingResetNameDialog = true
                                } label: {
                                    Label("EDIT NAME", systemImage: "person.text.rectangle")
                                        .frame(maxWidth: .infinity)
                                }
                                .buttonStyle(PixelButtonStyle())
                                .controlSize(.regular)
                            }
                        }

                        HStack {
                            Text("DANGER")
                                .font(.system(size: 10, weight: .heavy, design: .rounded))
                                .foregroundStyle(Color(red: 1.0, green: 0.45, blue: 0.45))
                            Spacer()
                        }

                        HStack(spacing: 8) {
                            Button(role: .destructive) {
                                SoundEffects.shared.play(.menuButton)
                                LeaveLobby.invoke()
                                showingResetNameDialog = false
                                vm.isMenuOpen = false
                            } label: {
                                Label("LEAVE LOBBY", systemImage: "rectangle.portrait.and.arrow.right")
                                    .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(PixelButtonStyle(danger: true))
                            .controlSize(.regular)
                            .disabled(vm.myLobby == nil)

                            Button(role: .destructive) {
                                SoundEffects.shared.play(.menuButton)
                                EndMatch.invoke()
                                showingResetNameDialog = false
                                vm.isMenuOpen = false
                            } label: {
                                Label("END MATCH", systemImage: "flag.checkered")
                                    .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(PixelButtonStyle(danger: true))
                            .controlSize(.regular)
                            .disabled(!vm.isPlaying)
                        }

                        Button(role: .destructive) {
                            SoundEffects.shared.play(.menuButton)
                            showingResetNameDialog = false
                            vm.stop()
                            onExit?(.quit)
                        } label: {
                            Label("RETURN TO TITLE", systemImage: "xmark.circle")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(PixelButtonStyle(danger: true))
                        .controlSize(.regular)

                        if showingResetNameDialog {
                            VStack(alignment: .leading, spacing: 8) {
                                Text("Edit Name")
                                    .font(.system(size: 12, weight: .heavy, design: .rounded))
                                    .foregroundStyle(.white)
                                Text("Update your callsign without leaving this session.")
                                    .font(.system(size: 10, weight: .medium, design: .rounded))
                                    .foregroundStyle(Color(white: 0.44))

                                TextField("NEW CALLSIGN", text: $resetNameDraft)
                                    .textFieldStyle(.plain)
                                    .font(.system(size: 13, weight: .bold, design: .rounded))
                                    .foregroundColor(.white)
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 8)
                                    .background(Color.white.opacity(0.06))
                                    .overlay(Rectangle().strokeBorder(Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.40), lineWidth: 2))
                                    .onSubmit {
                                        let trimmed = resetNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
                                        guard !trimmed.isEmpty else { return }
                                        SoundEffects.shared.play(.buttonPress)
                                        vm.renameCurrentPlayer(to: trimmed)
                                        showingResetNameDialog = false
                                    }

                                HStack(spacing: 8) {
                                    Button("Back") {
                                        SoundEffects.shared.play(.buttonPress)
                                        showingResetNameDialog = false
                                    }
                                    .buttonStyle(PixelButtonStyle())

                                    Button("Save") {
                                        let trimmed = resetNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
                                        guard !trimmed.isEmpty else { return }
                                        SoundEffects.shared.play(.buttonPress)
                                        vm.renameCurrentPlayer(to: trimmed)
                                        showingResetNameDialog = false
                                    }
                                    .buttonStyle(PixelButtonStyle(filled: true))
                                    .disabled(resetNameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(10)
                            .background(Color.white.opacity(0.06))
                            .overlay(Rectangle().strokeBorder(Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.26), lineWidth: 2))
                        }
                    }
                    .frame(maxWidth: 560)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 16)
                    .pixelPanel()
                    .shadow(color: Color(red: 0.3, green: 0.6, blue: 1.0).opacity(0.20), radius: 24, x: 0, y: 8)
                }
                .transition(.opacity.combined(with: .scale(scale: 0.98)))
            }
        }
        .animation(.spring(duration: 0.3), value: vm.isMenuOpen)
        .animation(.easeInOut(duration: 1.5), value: vm.isDead) // Smooth B&W transition
        .onChange(of: vm.hasJoined) { _, _ in onMusicChange?(isActivePlayState) }
        .onChange(of: vm.isDead) { _, _ in onMusicChange?(isActivePlayState) }
        .onChange(of: vm.isMenuOpen) { _, _ in onMusicChange?(isActivePlayState) }
        .onChange(of: isMuted) { _, newVal in SoundEffects.shared.isMuted = newVal }
        .disabled(isBackground)
        .onAppear {
            vm.start()
            onMusicChange?(isActivePlayState)

            if !isBackground {
                #if canImport(AppKit)
                NSApp.activate(ignoringOtherApps: true)
                DispatchQueue.main.async {
                    NSApp.windows.first?.makeKeyAndOrderFront(nil)
                }
                vm.installKeyboardMonitor()
                #endif
            }
        }
        .onDisappear {
            if !isBackground {
                vm.uninstallKeyboardMonitor()
            }
            if ownsViewModel {
                vm.stop()
            }
        }
    }

    private var statusBar: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                HStack(spacing: 6) {
                    Rectangle()
                        .fill(vm.isConnected ? Color.green : Color.red)
                        .frame(width: 7, height: 7)
                    Text(vm.isConnected ? "ONLINE" : "OFFLINE")
                        .font(.system(size: 10, weight: .heavy, design: .rounded))
                        .foregroundStyle(vm.isConnected ? Color.green : Color.red)
                }
                .padding(.horizontal, 9)
                .padding(.vertical, 5)
                .background(Color.white.opacity(0.06))
                .overlay(Rectangle().strokeBorder(Color(white: 0.28), lineWidth: 1))

                if let me = vm.myPlayer {
                    HStack(spacing: 5) {
                        Text("►").foregroundStyle(SurvivorsTheme.accent)
                        Text(me.name)
                    }
                    .font(.system(size: 11, weight: .heavy, design: .rounded))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .padding(.horizontal, 9)
                    .padding(.vertical, 5)
                    .background(Color.white.opacity(0.06))
                    .overlay(Rectangle().strokeBorder(Color(white: 0.28), lineWidth: 1))
                }

                Spacer(minLength: 8)

                if let me = vm.myPlayer {
                    HudHealthMeter(health: me.health)
                        .frame(width: 160)
                }

                HudStatChip(label: "Kills", value: "\(vm.myPlayer?.kills ?? 0)", tint: .orange)
                HudStatChip(label: "Swords", value: "\(vm.myPlayer?.weaponCount ?? 0)", tint: SurvivorsTheme.accent)
                HudStatChip(label: "Players", value: "\(vm.players.count)", tint: .green)

                if let onMuteToggle = onMuteToggle {
                    Button {
                        if isMuted { SoundEffects.shared.play(.muteToggle) }
                        onMuteToggle()
                    } label: {
                        Label(isMuted ? "Muted" : "Audio", systemImage: isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                            .labelStyle(.iconOnly)
                            .frame(width: 28, height: 28)
                            .background(Color.white.opacity(0.08))
                            .overlay(Rectangle().strokeBorder(Color(white: 0.28), lineWidth: 1))
                    }
                    .buttonStyle(.plain)
                    .help(isMuted ? "Unmute" : "Mute")
                }
            }

            if !vm.connectionDetail.isEmpty {
                Text(vm.connectionDetail)
                    .font(.system(size: 9, weight: .medium, design: .rounded))
                    .foregroundStyle(vm.isConnected ? Color(white: 0.42) : Color.red)
                    .lineLimit(1)
                    .padding(.horizontal, 2)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(
            Rectangle()
                .fill(Color(red: 0.07, green: 0.04, blue: 0.16).opacity(0.94))
                .overlay(
                    Rectangle()
                        .strokeBorder(Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.38), lineWidth: 2)
                )
        )
        .padding(.top, 0)
        .shadow(color: Color(red: 0.3, green: 0.6, blue: 1.0).opacity(0.12), radius: 8, x: 0, y: 4)
    }

    private var playingFooter: some View {
        HStack(spacing: 8) {
            if vm.initialName == nil {
                Button {
                    SoundEffects.shared.play(.buttonPress)
                    vm.ensureIdentityRegistered(allowFallback: true)
                } label: {
                    Text("Join")
                }
                .buttonStyle(PixelButtonStyle(filled: true))
                .controlSize(.small)
            }

            Button {
                SoundEffects.shared.play(.buttonPress)
                SpawnTestPlayer.invoke()
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: "figure.2.and.child.holdinghands")
                    Text("Spawn Bot")
                }
            }
            .buttonStyle(PixelButtonStyle())
            .controlSize(.small)

            Text("|")
                .font(.system(size: 11, weight: .heavy, design: .rounded))
                .foregroundStyle(Color(white: 0.25))

            HStack(spacing: 10) {
                HStack(spacing: 5) {
                    Image(systemName: "person.3.fill")
                    Text(vm.activeLobbyId.map { "LOBBY #\($0)" } ?? "NO LOBBY")
                }
                HStack(spacing: 5) {
                    Image(systemName: "dot.radiowaves.left.and.right")
                    Text("\(vm.players.count) ONLINE")
                }
            }
            .font(.system(size: 10, weight: .bold, design: .rounded))
            .foregroundStyle(Color(white: 0.44))

            Spacer(minLength: 12)

            Text("WASD / Arrows  •  Esc: Menu")
                .font(.system(size: 10, weight: .medium, design: .rounded))
                .foregroundStyle(Color(white: 0.32))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .frame(maxWidth: 920)
        .background(
            Rectangle()
                .fill(Color(red: 0.07, green: 0.04, blue: 0.16).opacity(0.94))
                .overlay(
                    Rectangle()
                        .strokeBorder(Color(red: 0.55, green: 0.82, blue: 1.0).opacity(0.38), lineWidth: 2)
                )
        )
        .shadow(color: Color(red: 0.3, green: 0.6, blue: 1.0).opacity(0.10), radius: 8, x: 0, y: -2)
    }
    
    // MARK: - Overlays Removed
}

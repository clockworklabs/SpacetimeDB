import SwiftUI
import Observation
import SpacetimeDB
#if canImport(AppKit)
import AppKit
#endif

// MARK: - View Model

@MainActor
@Observable
public class NinjaGameViewModel: SpacetimeClientDelegate {
    static let maxPlayersPerLobby = 30

    public enum NinjaDirection {
        case north, south, east, west
    }

    public var environment: SpacetimeEnvironment = .local
    var players: [Player] = []
    private var allPlayers: [Player] = []
    private var playersById: [UInt64: Player] = [:]
    private var playersByLobby: [UInt64: [Player]] = [:]
    private var playerCountsByLobby: [UInt64: Int] = [:]
    private var playersInActiveLobbySnapshot: [Player] = []
    private var lobbiesSnapshot: [Lobby] = []
    var weapons: [WeaponDrop] = []
    var isConnected = false
    var userId: UInt64?
    var connectionDetail: String = ""
    var initialName: String?
    private var client: SpacetimeClient?
    var recentEvents: [GameEventEntry] = []
    var renderPlayers: [Player] = []
    
    
    /// Tracks previous player states to detect drops in health or increases in kills.
    private var lastPlayerStates: [UInt64: (health: UInt32, kills: UInt32, weaponCount: UInt32)] = [:]
    
    /// Tracks which way each player is currently facing for animation.
    var playerDirections: [UInt64: NinjaDirection] = [:]
    /// Tracks if a remote player was moving in the last tick to trigger walk anims.
    var playerIsMoving: [UInt64: Bool] = [:]
    
    // Derived from the active Lobby row for current player
    var lobbies: [Lobby] {
        lobbiesSnapshot
    }
    
    // Stable lobby identity for UI/screen routing during short replica gaps.
    private var stableLobbyId: UInt64?

    var activeLobbyId: UInt64? {
        myPlayer?.lobbyId ?? stableLobbyId
    }

    var myLobby: Lobby? {
        guard let lobbyId = activeLobbyId else { return nil }
        return lobbies.first(where: { $0.id == lobbyId })
    }

    func playerCount(forLobbyId lobbyId: UInt64) -> Int {
        playerCountsByLobby[lobbyId] ?? 0
    }

    func lobbyIsFull(_ lobby: Lobby) -> Bool {
        playerCount(forLobbyId: lobby.id) >= Self.maxPlayersPerLobby
    }
    
    var isPlaying: Bool {
        myLobby?.isPlaying ?? false
    }

    var myPlayer: Player? {
        guard let userId else { return nil }
        return playersById[userId]
    }
    
    var playersInMyLobby: [Player] {
        guard let lobbyId = activeLobbyId else { return [] }
        if myPlayer?.lobbyId == lobbyId {
            return playersInActiveLobbySnapshot
        }
        return playersByLobby[lobbyId] ?? []
    }
    
    var isDead: Bool { myPlayer?.health == 0 }

    // Local position for client-side prediction (and camera anchor)
    var localX: Float = 500
    var localY: Float = 500

    // Joystick state
    var jsActive = false
    var jsBase: CGPoint?
    var jsVector: CGVector = .zero

    // Keyboard state
    private var pressedKeys: Set<UInt16> = []
    private var weaponSpawnTimer: Timer?
    private var isStarted = false
    // Don't send movement until the player has joined
    var hasJoined = false
    var isMenuOpen = false {
        didSet {
            let sound: SoundEffects.Sound = isMenuOpen ? .menuOpen : .menuClose
            SoundEffects.shared.play(sound)
            if isMenuOpen {
                // Don't keep movement keys "held" when opening an input-driven menu.
                pressedKeys.removeAll()
            }
        }
    }
    private var previousWeaponCount: UInt32 = 0
    private var previousHealth: UInt32 = 100
    /// Last time each target took damage from any of my swords.
    private var lastSwordHitTime: [UInt64: TimeInterval] = [:]
    private let swordHitCooldown: TimeInterval = 0.125
    private var lastSwordCollisionSweepTime: TimeInterval = 0
    private let swordCollisionSweepInterval: TimeInterval = 1.0 / 20.0
    private struct SwordOffset: Sendable {
        let x: Float
        let y: Float
    }
    private struct CollisionTargetSnapshot: Sendable {
        let id: UInt64
        let x: Float
        let y: Float
        let lastHitTime: TimeInterval
    }
    private struct SwordCollisionSnapshot: Sendable {
        let myX: Float
        let myY: Float
        let now: TimeInterval
        let cooldown: TimeInterval
        let swordOffsets: [SwordOffset]
        let targets: [CollisionTargetSnapshot]
    }
    private let collisionComputeQueue = DispatchQueue(
        label: "ninjagame.sword-collision.compute",
        qos: .utility
    )
    private var collisionComputeInFlight = false
    private let maxSwordAttacksPerSweep = max(
        1,
        Int(ProcessInfo.processInfo.environment["NINJA_MAX_ATTACKS_PER_SWEEP"] ?? "") ?? 8
    )
    /// Hit distance: player body radius (~20 pt) + sword-tip radius (~8 pt).
    private let swordHitRadius: Float = 28
    private let movementSpeedPerSecond: Float = 180
    private let movementTickInterval: TimeInterval = 1.0 / 60.0
    private let joystickRadius: CGFloat = 50
    private let joystickDeadzone: CGFloat = 5
    private let playerClampPadding: Float = playerEdgePadding
    private let networkSendRate: TimeInterval = 1.0 / 20.0  // Send position to server at 20Hz
    private var lastNetworkSend: Date = .distantPast
    private var movementTimer: Timer?
    private var lastMovementTick: TimeInterval = Date.timeIntervalSinceReferenceDate
    private var localPositionDirty = false  // Track if we need to send a network update
    private enum PendingLobbyAction {
        case create(name: String)
        case join(lobbyId: UInt64)
        case quickJoin(waitForLobbySnapshot: Bool, attemptsRemaining: Int)
    }
    private var pendingLobbyAction: PendingLobbyAction?
    private var pendingLobbyRetryWorkItem: DispatchWorkItem?
    var isQuickJoinActive = false
    private var lastReconnectAttemptAt: TimeInterval = 0
    private var pendingQuickJoinFromTitle = false
    private let lobbyActionRetryDelay: TimeInterval = 0.35
    private let lobbyActionMaxRetries = 20
    private var missingLobbyIdDetected: UInt64?
    private var missingLobbySince: TimeInterval = 0
    private var lastIdentityRepairAttempt: TimeInterval = 0
    private var eventSequence: Int = 0
    private var eventSnapshotLobbyId: UInt64?
    private var eventSnapshotPlayersById: [UInt64: Player] = [:]
    var smoothedPositions: [UInt64: (x: Float, y: Float)] = [:]
    private let verboseNetworkLogging = false
    private var lastRenderOrderUpdateTime: TimeInterval = 0
    private let renderOrderUpdateInterval: TimeInterval = 1.0 / 15.0
    private var renderPlayersDirty = false
    private enum HotPathSection: Int, CaseIterable {
        case onTransactionTotal
        case onTransactionRebuildCaches
        case onTransactionEvents
        case onTransactionEffects
        case tickMovementSwordCollision

        var label: String {
            switch self {
            case .onTransactionTotal:
                return "onTransactionUpdate.total"
            case .onTransactionRebuildCaches:
                return "onTransactionUpdate.rebuildCaches"
            case .onTransactionEvents:
                return "onTransactionUpdate.events"
            case .onTransactionEffects:
                return "onTransactionUpdate.effects"
            case .tickMovementSwordCollision:
                return "tickMovement.swordCollision"
            }
        }

        var marksSampleBoundary: Bool {
            self == .onTransactionTotal
        }
    }

    @ObservationIgnored private let perf = HotPathProfiler(
        enabled: ProcessInfo.processInfo.environment["NINJA_PROFILE_VM"] == "1",
        reportEverySamples: max(
            30,
            Int(ProcessInfo.processInfo.environment["NINJA_PROFILE_VM_WINDOW"] ?? "") ?? 180
        )
    )

    private final class HotPathProfiler {
        let enabled: Bool
        private let reportEverySamples: Int
        private var sectionTotalsNs: [UInt64]
        private var sectionCounts: [UInt32]
        private var sectionMaxNs: [UInt64]
        private var sampleCount: Int = 0

        init(enabled: Bool, reportEverySamples: Int) {
            self.enabled = enabled
            self.reportEverySamples = reportEverySamples
            let sectionCount = HotPathSection.allCases.count
            self.sectionTotalsNs = Array(repeating: 0, count: sectionCount)
            self.sectionCounts = Array(repeating: 0, count: sectionCount)
            self.sectionMaxNs = Array(repeating: 0, count: sectionCount)
        }

        @inline(__always)
        func measure<T>(_ section: HotPathSection, _ block: () -> T) -> T {
            guard enabled else { return block() }
            let start = DispatchTime.now().uptimeNanoseconds
            let result = block()
            let elapsed = DispatchTime.now().uptimeNanoseconds - start
            let index = section.rawValue
            sectionTotalsNs[index] &+= elapsed
            sectionCounts[index] &+= 1
            if elapsed > sectionMaxNs[index] {
                sectionMaxNs[index] = elapsed
            }
            if section.marksSampleBoundary {
                sampleCount += 1
                if sampleCount >= reportEverySamples {
                    reportAndReset()
                }
            }
            return result
        }

        func flushIfNeeded(reason: String) {
            guard enabled, sampleCount > 0 else { return }
            reportAndReset(reason: reason)
        }

        private func reportAndReset() {
            reportAndReset(reason: "window")
        }

        private func reportAndReset(reason: String) {
            guard enabled else { return }
            let samples = max(1, sampleCount)
            var rows: [String] = []
            rows.reserveCapacity(HotPathSection.allCases.count)
            for section in HotPathSection.allCases {
                let index = section.rawValue
                let count = Int(sectionCounts[index])
                guard count > 0 else { continue }
                let totalNs = sectionTotalsNs[index]
                let avgMs = (Double(totalNs) / Double(count)) / 1_000_000.0
                let maxMs = Double(sectionMaxNs[index]) / 1_000_000.0
                let perSampleMs = (Double(totalNs) / Double(samples)) / 1_000_000.0
                rows.append(
                    "\(section.label):avg=\(String(format: "%.3f", avgMs))ms max=\(String(format: "%.3f", maxMs))ms perSample=\(String(format: "%.3f", perSampleMs))ms n=\(count)"
                )
            }
            let summary = rows.joined(separator: " | ")
            print("[NinjaGame][Profile][\(reason)][samples=\(sampleCount)] \(summary)")
            sectionTotalsNs = Array(repeating: 0, count: sectionTotalsNs.count)
            sectionCounts = Array(repeating: 0, count: sectionCounts.count)
            sectionMaxNs = Array(repeating: 0, count: sectionMaxNs.count)
            sampleCount = 0
        }
    }

    private var isMovementInputActive: Bool {
        let dist = sqrt(jsVector.dx * jsVector.dx + jsVector.dy * jsVector.dy)
        return dist > joystickDeadzone || !pressedKeys.isEmpty
    }

    init(initialName: String? = nil) {
        self.initialName = initialName
        SpacetimeModule.registerTables()
    }

    // MARK: - Connection

    func start() {
        guard !isStarted else { return }
        isStarted = true

        let newClient = SpacetimeClient(
            serverUrl: environment.url,
            moduleName: "ninjagame"
        )
        newClient.delegate = self
        self.client = newClient
        SpacetimeClient.shared = newClient
        newClient.connect()

        startMovementTimer()
        startWeaponSpawner()
    }

    func stop() {
        guard isStarted else { return }
        let shouldSendLeave = hasJoined
        perf.flushIfNeeded(reason: "stop")
        isConnected = false
        movementTimer?.invalidate()
        movementTimer = nil
        weaponSpawnTimer?.invalidate()
        weaponSpawnTimer = nil
        #if canImport(AppKit)
        removeKeyboardMonitors()
        #endif
        isStarted = false
        hasJoined = false
        clearPendingLobbyAction()
        pendingQuickJoinFromTitle = false
        missingLobbyIdDetected = nil
        missingLobbySince = 0
        lastIdentityRepairAttempt = 0
        stableLobbyId = nil
        eventSnapshotLobbyId = nil
        eventSnapshotPlayersById.removeAll()
        clearGameState()
        if let client = self.client {
            client.delegate = nil
            if shouldSendLeave {
                Leave.invoke()
                // Give the outbound reducer message one short run-loop window
                // to flush before the websocket is closed.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
                    client.disconnect()
                }
            } else {
                client.disconnect()
            }
            if SpacetimeClient.shared === client {
                SpacetimeClient.shared = nil
            }
        }
        self.client = nil
    }

    /// Clears the SpacetimeDB table caches and resets all local game state.
    /// Called on both deliberate stop and unexpected disconnect so that stale
    /// player/weapon rows from a previous session are never shown on reconnect.
    private func clearGameState() {
        // Only clear shared caches if this VM owns the active client,
        // preventing a stale/background VM from wiping another VM's data.
        if SpacetimeClient.shared === client || SpacetimeClient.shared == nil {
            PlayerTable.cache.clear()
            WeaponDropTable.cache.clear()
            LobbyTable.cache.clear()
        }
        players = []
        allPlayers = []
        playersById.removeAll()
        playersByLobby.removeAll()
        playerCountsByLobby.removeAll()
        playersInActiveLobbySnapshot = []
        lobbiesSnapshot = []
        weapons = []
        userId = nil
        isMenuOpen = false
        previousWeaponCount = 0
        previousHealth = 100
        lastSwordHitTime.removeAll()
        lastSwordCollisionSweepTime = 0
        playerDirections.removeAll()
        playerIsMoving.removeAll()
        smoothedPositions.removeAll()
        pressedKeys.removeAll()
        jsActive = false
        jsBase = nil
        jsVector = .zero
        localX = 500
        localY = 500
        localPositionDirty = false
        lastNetworkSend = .distantPast
        isQuickJoinActive = false
        recentEvents = []
        eventSequence = 0
        lastPlayerStates.removeAll()
        renderPlayers = []
        lastRenderOrderUpdateTime = 0
        renderPlayersDirty = false
    }

    // MARK: - SpacetimeClientDelegate

    public func onConnect() {
        guard isStarted else { return }
        isConnected = true
        connectionDetail = ""
        // Set player name after identifying with the server.
        // If initialName is nil (Reconnect flow), don't force rename.
        ensureIdentityRegistered(allowFallback: false)
        performPendingQuickJoinIfNeeded()
    }

    public func onDisconnect(error: Error?) {
        guard isStarted else { return }
        perf.flushIfNeeded(reason: "disconnect")
        isConnected = false
        hasJoined = false
        clearPendingLobbyAction()
        pendingQuickJoinFromTitle = false
        missingLobbyIdDetected = nil
        missingLobbySince = 0
        stableLobbyId = nil
        eventSnapshotLobbyId = nil
        eventSnapshotPlayersById.removeAll()
        if let error {
            connectionDetail = error.localizedDescription
            print("[NinjaGame] onDisconnect(error): \(error.localizedDescription)")
        } else {
            connectionDetail = ""
            print("[NinjaGame] onDisconnect(clean)")
        }
        clearGameState()
    }

    public func onIdentityReceived(identity: [UInt8], token: String) {
        guard identity.count >= 8 else { return }
        self.userId = identity.withUnsafeBytes { $0.loadUnaligned(as: UInt64.self).littleEndian }
        print("[NinjaGame] userId set to: \(String(format: "%016llx", self.userId!))")
    }

    public func onTransactionUpdate(message: Data?) {
        perf.measure(.onTransactionTotal) {
            let pTable = PlayerTable.cache.rows
            let now = Date.timeIntervalSinceReferenceDate

            perf.measure(.onTransactionRebuildCaches) {
                allPlayers = pTable

                playersById.removeAll(keepingCapacity: true)
                playersById.reserveCapacity(pTable.count)
                playersByLobby.removeAll(keepingCapacity: true)
                playerCountsByLobby.removeAll(keepingCapacity: true)

                for p in pTable {
                    playersById[p.id] = p
                    if let lobbyId = p.lobbyId {
                        playersByLobby[lobbyId, default: []].append(p)
                        playerCountsByLobby[lobbyId, default: 0] += 1
                    }
                }

                lobbiesSnapshot = LobbyTable.cache.rows.sorted { $0.id < $1.id }
            }

            if verboseNetworkLogging {
                let idList = pTable.map { String(format: "%016llx", $0.id) }.joined(separator: ", ")
                print("[NinjaGame] onTransactionUpdate: userId=\(userId.map { String(format:"%016llx",$0) } ?? "nil") players=[\(idList)] hasJoined=\(hasJoined)")
            }

            if let myId = userId, let me = playersById[myId] {
                let serverX = clampToWorld(me.x, padding: playerClampPadding)
                let serverY = clampToWorld(me.y, padding: playerClampPadding)
                if me.lobbyId != nil {
                    clearPendingLobbyAction()
                    let transientPrefixes = [
                        "Looking for open lobbies",
                        "No open lobbies found; creating ",
                        "Joining lobby",
                        "Creating lobby",
                        "Registering player as ",
                    ]
                    if transientPrefixes.contains(where: { connectionDetail.hasPrefix($0) }) {
                        connectionDetail = ""
                    }
                }

                if let lobbyId = me.lobbyId {
                    stableLobbyId = lobbyId
                    let lobbyExists = lobbiesSnapshot.contains(where: { $0.id == lobbyId })
                    if lobbyExists {
                        missingLobbyIdDetected = nil
                        missingLobbySince = 0
                    } else if missingLobbyIdDetected != lobbyId {
                        missingLobbyIdDetected = lobbyId
                        missingLobbySince = now
                    } else if now - missingLobbySince > 1.2 {
                        connectionDetail = "Lobby closed; returning to browser…"
                        LeaveLobby.invoke()
                        missingLobbyIdDetected = nil
                        missingLobbySince = 0
                    }
                } else {
                    stableLobbyId = nil
                    missingLobbyIdDetected = nil
                    missingLobbySince = 0
                }

                if !hasJoined {
                    // First time we see ourselves — snap camera to server position
                    hasJoined = true
                    localX = serverX
                    localY = serverY
                    previousWeaponCount = me.weaponCount
                    previousHealth = me.health
                } else {
                    let respawned = me.health > previousHealth
                    let driftX = abs(localX - serverX)
                    let driftY = abs(localY - serverY)
                    if respawned || driftX > 50 || driftY > 50 {
                        // Hard snap on respawn or teleport-level drift.
                        localX = serverX
                        localY = serverY
                    } else if !isMovementInputActive && (driftX > 1 || driftY > 1) {
                        // Only correct small drift when user isn't actively moving,
                        // which avoids visible rubber-banding while walking.
                        localX += (serverX - localX) * 0.2
                        localY += (serverY - localY) * 0.2
                    }

                    if me.weaponCount > previousWeaponCount {
                        SoundEffects.shared.play(.weaponPickup)
                    }
                    previousWeaponCount = me.weaponCount

                    if me.health == 0 && previousHealth > 0 {
                        SoundEffects.shared.play(.death)
                    }
                    previousHealth = me.health
                }
                performPendingLobbyActionIfReady()
            } else if userId != nil && hasJoined {
                // Self row temporarily missing from replica. Try to self-heal once every 2s.
                hasJoined = false
                stableLobbyId = nil
                if now - lastIdentityRepairAttempt > 2.0 {
                    lastIdentityRepairAttempt = now
                    connectionDetail = "Player row missing; re-registering…"
                    ensureIdentityRegistered(allowFallback: true)
                }
            }

            let lobbyId = activeLobbyId
            let scopedPlayers: [Player]
            if let lobbyId {
                scopedPlayers = playersByLobby[lobbyId] ?? []
            } else {
                scopedPlayers = pTable
            }

            perf.measure(.onTransactionEvents) {
                processLobbyEvents(lobbyPlayers: scopedPlayers, activeLobbyId: lobbyId)
            }

            players = scopedPlayers
            playersInActiveLobbySnapshot = scopedPlayers

            if let lobbyId {
                var filteredWeapons: [WeaponDrop] = []
                filteredWeapons.reserveCapacity(WeaponDropTable.cache.rows.count)
                for w in WeaponDropTable.cache.rows where w.lobbyId == lobbyId {
                    filteredWeapons.append(w)
                }
                weapons = filteredWeapons
            } else {
                weapons = WeaponDropTable.cache.rows
            }

            renderPlayersDirty = true
            refreshRenderPlayersIfNeeded(now: now)

            perf.measure(.onTransactionEffects) {
                var seenIds = Set<UInt64>()
                seenIds.reserveCapacity(pTable.count)

                for p in pTable {
                    seenIds.insert(p.id)
                    guard let last = lastPlayerStates[p.id] else {
                        lastPlayerStates[p.id] = (p.health, p.kills, p.weaponCount)
                        continue
                    }

                    if p.health < last.health && p.health > 0 {
                        EffectManager.shared.spawnHit(x: p.x, y: p.y, value: "-\(last.health - p.health)")
                    }

                    if p.health == 0 && last.health > 0 {
                        EffectManager.shared.spawnDeath(x: p.x, y: p.y)
                    }

                    if p.kills > last.kills {
                        EffectManager.shared.spawnKill(x: p.x, y: p.y)
                    }

                    if p.weaponCount > last.weaponCount {
                        EffectManager.shared.spawnPickup(x: p.x, y: p.y, value: "+1 SWORD")
                    }

                    lastPlayerStates[p.id] = (p.health, p.kills, p.weaponCount)
                }

                if lastPlayerStates.count > seenIds.count {
                    let staleIds = lastPlayerStates.keys.filter { !seenIds.contains($0) }
                    for id in staleIds {
                        lastPlayerStates.removeValue(forKey: id)
                    }
                }
            }
        }
    }

    public func onReducerError(reducer: String, message: String, isInternal: Bool) {
        let lowered = message.lowercased()
        if lowered.contains("no such reducer") {
            connectionDetail = "missing reducer '\(reducer)' on server; publish ninjagame module"
        } else {
            connectionDetail = "\(isInternal ? "internal" : "reducer") error (\(reducer))"
        }
        print("[NinjaGame] reducer error for '\(reducer)': \(message)")
    }

    func ensureIdentityRegistered(allowFallback: Bool) {
        let trimmedInitial = initialName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !trimmedInitial.isEmpty {
            SetName.invoke(name: trimmedInitial)
            return
        }

        guard allowFallback else { return }

        if let currentName = myPlayer?.name,
           !currentName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return
        }

        let fallbackName: String
        if let userId {
            fallbackName = "Player \(String(format: "%04X", userId & 0xFFFF))"
        } else {
            fallbackName = "Player \(Int.random(in: 1...9999))"
        }
        initialName = fallbackName
        connectionDetail = "Registering player as \(fallbackName)…"
        SetName.invoke(name: fallbackName)
    }

    func renameCurrentPlayer(to name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        initialName = trimmed
        connectionDetail = "Updating name to \(trimmed)…"
        SetName.invoke(name: trimmed)
    }

    func scheduleQuickJoinFromTitle() {
        pendingQuickJoinFromTitle = true
        performPendingQuickJoinIfNeeded()
    }

    func clearPendingQuickJoinFromTitle() {
        pendingQuickJoinFromTitle = false
    }

    private func performPendingQuickJoinIfNeeded() {
        guard pendingQuickJoinFromTitle, isConnected else { return }
        pendingQuickJoinFromTitle = false
        quickJoinFirstLobbyWithRetry(waitForLobbySnapshot: true, attemptsRemaining: 6)
    }
    
    func refreshLobbies() {
        guard let client = client else { return }
        guard isStarted else { return }
        SoundEffects.shared.play(.buttonPress)
        connectionDetail = "Refreshing connection..."
        client.disconnect()
        // Small delay to allow disconnect to settle before reconnecting
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in
            guard let self else { return }
            guard self.isStarted, self.client === client else { return }
            client.connect()
        }
    }

    private func clearPendingLobbyAction() {
        pendingLobbyAction = nil
        pendingLobbyRetryWorkItem?.cancel()
        pendingLobbyRetryWorkItem = nil
    }

    private func queuePendingLobbyAction(_ action: PendingLobbyAction, detail: String) {
        pendingLobbyAction = action
        connectionDetail = detail
        ensureIdentityRegistered(allowFallback: true)
        schedulePendingLobbyActionRetry(attemptsRemaining: lobbyActionMaxRetries)
    }

    private func schedulePendingLobbyActionRetry(attemptsRemaining: Int) {
        pendingLobbyRetryWorkItem?.cancel()

        guard pendingLobbyAction != nil else { return }
        guard isStarted else {
            clearPendingLobbyAction()
            return
        }
        guard attemptsRemaining > 0 else {
            recoverConnectionForPendingLobbyAction()
            return
        }

        let workItem = DispatchWorkItem { [weak self] in
            guard let self else { return }
            guard self.pendingLobbyAction != nil else { return }
            guard self.isStarted else {
                self.clearPendingLobbyAction()
                return
            }
            guard self.isConnected else {
                self.requestReconnectIfNeeded(detail: "Disconnected; reconnecting…")
                self.schedulePendingLobbyActionRetry(attemptsRemaining: attemptsRemaining - 1)
                return
            }
            guard self.myPlayer != nil, self.hasJoined else {
                self.ensureIdentityRegistered(allowFallback: true)
                self.schedulePendingLobbyActionRetry(attemptsRemaining: attemptsRemaining - 1)
                return
            }
            self.performPendingLobbyActionIfReady()
        }

        pendingLobbyRetryWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + lobbyActionRetryDelay, execute: workItem)
    }

    private func recoverConnectionForPendingLobbyAction() {
        guard pendingLobbyAction != nil else { return }
        guard isStarted else {
            clearPendingLobbyAction()
            return
        }
        guard let client else { return }

        connectionDetail = "Player row still missing; reconnecting…"
        pendingLobbyRetryWorkItem?.cancel()
        pendingLobbyRetryWorkItem = nil
        client.disconnect()
        client.delegate = self
        client.connect()
        schedulePendingLobbyActionRetry(attemptsRemaining: lobbyActionMaxRetries)
    }

    private func requestReconnectIfNeeded(detail: String) {
        guard isStarted else { return }
        connectionDetail = detail
        let now = Date.timeIntervalSinceReferenceDate
        if now - lastReconnectAttemptAt < 1.2 {
            return
        }
        lastReconnectAttemptAt = now
        print("[NinjaGame] reconnect requested: \(detail)")
        client?.connect()
    }

    private func performPendingLobbyActionIfReady() {
        guard isStarted else {
            clearPendingLobbyAction()
            return
        }
        guard let pending = pendingLobbyAction else { return }
        guard isConnected, hasJoined, let me = myPlayer else { return }
        guard me.lobbyId == nil else {
            clearPendingLobbyAction()
            return
        }

        clearPendingLobbyAction()
        switch pending {
        case .create(let name):
            createLobbyWithRetry(name: name, attemptsRemaining: lobbyActionMaxRetries)
        case .join(let lobbyId):
            joinLobbyWithRetry(lobbyId: lobbyId, attemptsRemaining: lobbyActionMaxRetries)
        case .quickJoin(let waitForLobbySnapshot, let attemptsRemaining):
            quickJoinFirstLobbyWithRetry(
                waitForLobbySnapshot: waitForLobbySnapshot,
                attemptsRemaining: attemptsRemaining
            )
        }
    }

    func createLobbyWithRetry(name: String, attemptsRemaining: Int? = nil) {
        guard isStarted else { return }
        guard isConnected else {
            requestReconnectIfNeeded(detail: "Disconnected; reconnecting…")
            return
        }
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        let attempts = attemptsRemaining ?? lobbyActionMaxRetries
        guard myPlayer?.lobbyId == nil else { return }

        if myPlayer == nil || !hasJoined {
            queuePendingLobbyAction(.create(name: trimmed), detail: "Creating lobby… waiting for player row.")
            return
        }

        clearPendingLobbyAction()
        connectionDetail = "Creating lobby…"
        CreateLobby.invoke(name: trimmed)

        guard attempts > 0 else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + lobbyActionRetryDelay) { [weak self] in
            guard let self else { return }
            guard self.isStarted else { return }
            guard self.isConnected, self.myPlayer?.lobbyId == nil else { return }
            self.createLobbyWithRetry(name: trimmed, attemptsRemaining: attempts - 1)
        }
    }

    func joinLobbyWithRetry(lobbyId: UInt64, attemptsRemaining: Int? = nil) {
        guard isStarted else { return }
        guard isConnected else {
            requestReconnectIfNeeded(detail: "Disconnected; reconnecting…")
            return
        }
        let attempts = attemptsRemaining ?? lobbyActionMaxRetries
        guard myPlayer?.lobbyId == nil else { return }

        if myPlayer == nil || !hasJoined {
            queuePendingLobbyAction(.join(lobbyId: lobbyId), detail: "Joining lobby… waiting for player row.")
            return
        }
        clearPendingLobbyAction()
        connectionDetail = "Joining lobby…"
        print("[NinjaGame] joinLobbyWithRetry invoke lobbyId=\(lobbyId), attempts=\(attempts)")
        JoinLobby.invoke(lobbyId: lobbyId)

        guard attempts > 0 else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + lobbyActionRetryDelay) { [weak self] in
            guard let self else { return }
            guard self.isStarted else { return }
            guard self.isConnected, self.myPlayer?.lobbyId == nil else { return }
            self.joinLobbyWithRetry(lobbyId: lobbyId, attemptsRemaining: attempts - 1)
        }
    }

    func quickJoinFirstLobbyWithRetry(waitForLobbySnapshot: Bool = false, attemptsRemaining: Int = 0) {
        guard isStarted else {
            isQuickJoinActive = false
            return
        }
        isQuickJoinActive = true
        guard isConnected else {
            requestReconnectIfNeeded(detail: "Disconnected; reconnecting…")
            return
        }
        guard myPlayer?.lobbyId == nil else {
            clearPendingLobbyAction()
            return
        }

        if myPlayer == nil || !hasJoined {
            queuePendingLobbyAction(
                .quickJoin(waitForLobbySnapshot: waitForLobbySnapshot, attemptsRemaining: attemptsRemaining),
                detail: "Joining lobby… waiting for player registration."
            )
            return
        }

        let candidateLobby =
            lobbies.first(where: { !$0.isPlaying && !lobbyIsFull($0) }) ??
            lobbies.first(where: { !lobbyIsFull($0) })

        guard let targetLobby = candidateLobby else {
            if waitForLobbySnapshot && attemptsRemaining > 0 {
                connectionDetail = "Looking for open lobbies…"
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.30) { [weak self] in
                    guard let self else { return }
                    guard self.isStarted else { return }
                    self.quickJoinFirstLobbyWithRetry(
                        waitForLobbySnapshot: true,
                        attemptsRemaining: attemptsRemaining - 1
                    )
                }
            } else {
                let baseName = (myPlayer?.name ?? initialName ?? "Player")
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                let lobbyName = baseName.isEmpty ? "Quick Lobby" : "\(baseName)'s Lobby"
                connectionDetail = "No open lobbies found; creating \(lobbyName)…"
                createLobbyWithRetry(name: lobbyName, attemptsRemaining: lobbyActionMaxRetries)
            }
            return
        }
        print("[NinjaGame] quickJoin target lobbyId=\(targetLobby.id) isPlaying=\(targetLobby.isPlaying)")
        joinLobbyWithRetry(lobbyId: targetLobby.id, attemptsRemaining: lobbyActionMaxRetries)
    }

    private func appendEvent(_ text: String, kind: GameEventEntry.Kind) {
        eventSequence += 1
        recentEvents.append(GameEventEntry(id: eventSequence, text: text, kind: kind, timestamp: Date()))
        if recentEvents.count > 30 {
            recentEvents.removeFirst(recentEvents.count - 30)
        }
    }

    private func processLobbyEvents(lobbyPlayers: [Player], activeLobbyId: UInt64?) {
        guard let lobbyId = activeLobbyId else {
            eventSnapshotLobbyId = nil
            eventSnapshotPlayersById.removeAll()
            return
        }

        var currentById: [UInt64: Player] = [:]
        currentById.reserveCapacity(lobbyPlayers.count)
        for player in lobbyPlayers {
            currentById[player.id] = player
        }

        // Prime snapshot when entering/changing lobbies to avoid noisy initial flood.
        guard eventSnapshotLobbyId == lobbyId else {
            eventSnapshotLobbyId = lobbyId
            eventSnapshotPlayersById = currentById
            return
        }

        let previousById = eventSnapshotPlayersById

        for player in lobbyPlayers where previousById[player.id] == nil {
            appendEvent("\(player.name) joined the lobby", kind: .info)
        }

        for (id, player) in previousById where currentById[id] == nil {
            appendEvent("\(player.name) left the lobby", kind: .info)
        }

        for (id, current) in currentById {
            guard let previous = previousById[id] else { continue }
            if current.kills > previous.kills {
                let delta = Int(current.kills - previous.kills)
                for _ in 0..<delta {
                    appendEvent("\(current.name) scored a kill", kind: .combat)
                }
            }
        }

        eventSnapshotPlayersById = currentById
    }

    // MARK: - Movement

    private func clampToWorld(_ value: Float, padding: Float = 0) -> Float {
        let minValue = worldMin + padding
        let maxValue = worldMax - padding
        return max(minValue, min(maxValue, value))
    }

    func randomWeaponSpawn() -> (x: Float, y: Float) {
        let minSpawn = worldMin + weaponSpawnPadding
        let maxSpawn = worldMax - weaponSpawnPadding
        return (
            Float.random(in: minSpawn...maxSpawn),
            Float.random(in: minSpawn...maxSpawn)
        )
    }

    private func moveBy(dx: Float, dy: Float) {
        guard hasJoined else { return }
        localX = clampToWorld(localX + dx, padding: playerClampPadding)
        localY = clampToWorld(localY + dy, padding: playerClampPadding)
        localPositionDirty = true
    }

    /// Send position to server at a throttled rate (20Hz)
    private func flushPositionIfNeeded() {
        guard localPositionDirty else { return }
        let now = Date()
        guard now.timeIntervalSince(lastNetworkSend) >= networkSendRate else { return }
        lastNetworkSend = now
        localPositionDirty = false
        MovePlayer.invoke(x: localX, y: localY)
    }

    func updateJoystick(active: Bool, base: CGPoint = .zero, current: CGPoint = .zero) {
        jsActive = active
        if active {
            jsBase = base
            let dx = current.x - base.x
            let dy = current.y - base.y
            let dist = sqrt(dx * dx + dy * dy)

            if dist > joystickRadius {
                jsVector = CGVector(dx: dx / dist * joystickRadius, dy: dy / dist * joystickRadius)
            } else {
                jsVector = CGVector(dx: dx, dy: dy)
            }
        } else {
            jsBase = nil
            jsVector = .zero
        }
    }

    private func startMovementTimer() {
        movementTimer?.invalidate()
        lastMovementTick = Date.timeIntervalSinceReferenceDate
        let timer = Timer(timeInterval: movementTickInterval, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.tickMovement(now: Date.timeIntervalSinceReferenceDate)
            }
        }
        movementTimer = timer
        RunLoop.main.add(timer, forMode: .common)
    }

    private func tickMovement(now: TimeInterval = Date.timeIntervalSinceReferenceDate) {
        guard hasJoined && !isMenuOpen else { return }
        let rawDt = now - lastMovementTick
        lastMovementTick = now
        let dt = Float(max(0, min(rawDt, 0.05)))
        
        // Smooth remote players: use exponential smoothing (EMA) for state-of-the-art lean netcode.
        // factor = 1 - pow(damping, dt * tickRate). 0.15 @ 60Hz is roughly 9 per second.
        let remoteSmoothingFactor = 1.0 - pow(0.001, dt) 
        var hasRemoteSmoothingUpdate = false
        let movingThresholdSq: Float = 0.05 * 0.05
        for p in players where p.id != userId {
            let current = smoothedPositions[p.id] ?? (p.x, p.y)
            let nextX = current.x + (p.x - current.x) * remoteSmoothingFactor
            let nextY = current.y + (p.y - current.y) * remoteSmoothingFactor
            smoothedPositions[p.id] = (nextX, nextY)

            let dx = nextX - current.x
            let dy = nextY - current.y
            let distSq = dx * dx + dy * dy
            if distSq > movingThresholdSq {
                playerIsMoving[p.id] = true
                if abs(dx) > abs(dy) {
                    playerDirections[p.id] = dx > 0 ? .east : .west
                } else {
                    playerDirections[p.id] = dy > 0 ? .south : .north
                }
                hasRemoteSmoothingUpdate = true
            } else {
                playerIsMoving[p.id] = false
            }
        }
        if hasRemoteSmoothingUpdate {
            renderPlayersDirty = true
        }
        refreshRenderPlayersIfNeeded(now: now)

        // Process published effect events (hand off to managers)
        // This will be handled by the UI listening to the VM's effectEvents array

        guard dt > 0 else {
            flushPositionIfNeeded()
            return
        }

        var inputX: Float = 0
        var inputY: Float = 0

        if jsActive && jsVector != .zero {
            let dist = sqrt(jsVector.dx * jsVector.dx + jsVector.dy * jsVector.dy)
            if dist > joystickDeadzone {
                inputX = Float(jsVector.dx / joystickRadius)
                inputY = Float(jsVector.dy / joystickRadius)
            }
        } else {
            // Keyboard: W=13, A=0, S=1, D=2, ←=123, →=124, ↓=125, ↑=126
            if pressedKeys.contains(13) || pressedKeys.contains(126) { inputY -= 1 }
            if pressedKeys.contains(1)  || pressedKeys.contains(125) { inputY += 1 }
            if pressedKeys.contains(0)  || pressedKeys.contains(123) { inputX -= 1 }
            if pressedKeys.contains(2)  || pressedKeys.contains(124) { inputX += 1 }
        }

        if inputX != 0 || inputY != 0 {
            let len = sqrt(inputX * inputX + inputY * inputY)
            let nx = inputX / max(1, len)
            let ny = inputY / max(1, len)
            let step = movementSpeedPerSecond * dt
            moveBy(dx: nx * step, dy: ny * step)
            renderPlayersDirty = true
            
            // Set my facing direction
            if let myId = userId {
                playerIsMoving[myId] = true
                if abs(inputX) > abs(inputY) {
                    playerDirections[myId] = inputX > 0 ? .east : .west
                } else {
                    playerDirections[myId] = inputY > 0 ? .south : .north
                }
            }
        } else if let myId = userId {
            playerIsMoving[myId] = false
        }

        // Check for sword-to-player collisions.
        perf.measure(.tickMovementSwordCollision) {
            checkSwordCollisions(now: now)
        }

        // Throttled network send so we don't flood the server
        flushPositionIfNeeded()
    }

    /// Checks whether any of my orbiting swords are touching another player.
    ///
    /// Uses the **same** `swordPositions(count:t:)` call as the renderer, so
    /// the hitboxes are pixel-perfect matches of what's drawn on screen.
    ///
    /// Coordinate space: world units == view points (the renderer draws world
    /// coords directly with no scale transform), so CGFloat sword offsets can
    /// be cast to Float and added to the Float world position without conversion.
    private func checkSwordCollisions(now: TimeInterval) {
        guard hasJoined, !isMenuOpen else { return }
        guard let myId = userId else { return }
        guard let me = myPlayer, me.weaponCount > 0, me.health > 0 else { return }
        guard players.count > 1 else { return }
        guard now - lastSwordCollisionSweepTime >= swordCollisionSweepInterval else { return }
        guard !collisionComputeInFlight else { return }
        lastSwordCollisionSweepTime = now

        let myX = localX
        let myY = localY

        var swordOffsets: [SwordOffset] = []
        swordOffsets.reserveCapacity(Int(me.weaponCount))
        forEachSwordPosition(count: Int(me.weaponCount), t: now) { offset in
            swordOffsets.append(SwordOffset(x: Float(offset.x), y: Float(offset.y)))
        }
        guard !swordOffsets.isEmpty else { return }

        var targets: [CollisionTargetSnapshot] = []
        targets.reserveCapacity(players.count - 1)
        for target in players where target.id != myId && target.health > 0 {
            targets.append(
                CollisionTargetSnapshot(
                    id: target.id,
                    x: target.x,
                    y: target.y,
                    lastHitTime: lastSwordHitTime[target.id] ?? -Double.infinity
                )
            )
        }
        guard !targets.isEmpty else { return }

        collisionComputeInFlight = true
        let snapshot = SwordCollisionSnapshot(
            myX: myX,
            myY: myY,
            now: now,
            cooldown: swordHitCooldown,
            swordOffsets: swordOffsets,
            targets: targets
        )
        let maxAttacksPerSweep = maxSwordAttacksPerSweep
        collisionComputeQueue.async { [snapshot, maxAttacksPerSweep] in
            let hits = Self.computeSwordCollisionHits(snapshot: snapshot, maxHits: maxAttacksPerSweep)
            Task { @MainActor [weak self] in
                guard let self else { return }
                self.collisionComputeInFlight = false
                guard self.hasJoined, !self.isMenuOpen else { return }
                guard !hits.isEmpty else { return }

                var didHit = false
                for targetId in hits {
                    // Re-validate cooldown at apply time to avoid duplicate sends
                    // when snapshots overlap with newer state.
                    let lastHit = self.lastSwordHitTime[targetId] ?? -Double.infinity
                    guard snapshot.now - lastHit >= self.swordHitCooldown else { continue }
                    self.lastSwordHitTime[targetId] = snapshot.now
                    Attack.invoke(targetId: targetId)
                    didHit = true
                }
                if didHit {
                    SoundEffects.shared.play(.attack)
                }
            }
        }
    }

    nonisolated private static func computeSwordCollisionHits(
        snapshot: SwordCollisionSnapshot,
        maxHits: Int
    ) -> [UInt64] {
        guard !snapshot.swordOffsets.isEmpty, !snapshot.targets.isEmpty else { return [] }
        guard maxHits > 0 else { return [] }

        struct SwordBounds {
            let left: Float
            let right: Float
            let top: Float
            let bottom: Float
        }

        // Pixel-perfect AABB (Axis-Aligned Bounding Box) dimensions (width / 2, height / 2)
        // Ninja is 36x42, Sword is 15x39
        let ninjaHalfW: Float = 18.0
        let ninjaHalfH: Float = 21.0
        let swordHalfW: Float = 7.5
        let swordHalfH: Float = 19.5

        var swordBounds: [SwordBounds] = []
        swordBounds.reserveCapacity(snapshot.swordOffsets.count)

        var maxSwordRadiusSq: Float = 0
        var swordsMinLeft = Float.greatestFiniteMagnitude
        var swordsMaxRight = -Float.greatestFiniteMagnitude
        var swordsMinTop = Float.greatestFiniteMagnitude
        var swordsMaxBottom = -Float.greatestFiniteMagnitude

        for offset in snapshot.swordOffsets {
            let sx = snapshot.myX + offset.x
            let sy = snapshot.myY + offset.y
            let bounds = SwordBounds(
                left: sx - swordHalfW,
                right: sx + swordHalfW,
                top: sy - swordHalfH,
                bottom: sy + swordHalfH
            )
            swordBounds.append(bounds)

            if bounds.left < swordsMinLeft { swordsMinLeft = bounds.left }
            if bounds.right > swordsMaxRight { swordsMaxRight = bounds.right }
            if bounds.top < swordsMinTop { swordsMinTop = bounds.top }
            if bounds.bottom > swordsMaxBottom { swordsMaxBottom = bounds.bottom }

            let radiusSq = offset.x * offset.x + offset.y * offset.y
            if radiusSq > maxSwordRadiusSq { maxSwordRadiusSq = radiusSq }
        }

        let maxSwordRadius = sqrt(maxSwordRadiusSq)
        let targetCullRadius = maxSwordRadius + ninjaHalfW + swordHalfH
        let targetCullRadiusSq = targetCullRadius * targetCullRadius

        // Coarse union bounds for all swords, expanded by target body size.
        let expandedLeft = swordsMinLeft - ninjaHalfW
        let expandedRight = swordsMaxRight + ninjaHalfW
        let expandedTop = swordsMinTop - ninjaHalfH
        let expandedBottom = swordsMaxBottom + ninjaHalfH

        var hitTargets: [UInt64] = []
        hitTargets.reserveCapacity(min(snapshot.targets.count, maxHits))

        for target in snapshot.targets {
            guard snapshot.now - target.lastHitTime >= snapshot.cooldown else { continue }

            let dxCenter = target.x - snapshot.myX
            let dyCenter = target.y - snapshot.myY
            let centerDistSq = dxCenter * dxCenter + dyCenter * dyCenter
            guard centerDistSq <= targetCullRadiusSq else { continue }

            // Target bounds
            let tLeft = target.x - ninjaHalfW
            let tRight = target.x + ninjaHalfW
            let tTop = target.y - ninjaHalfH
            let tBottom = target.y + ninjaHalfH

            guard tRight >= expandedLeft, tLeft <= expandedRight, tBottom >= expandedTop, tTop <= expandedBottom else {
                continue
            }

            for sword in swordBounds {
                if sword.left <= tRight &&
                    sword.right >= tLeft &&
                    sword.top <= tBottom &&
                    sword.bottom >= tTop {
                    hitTargets.append(target.id)
                    break
                }
            }

            if hitTargets.count >= maxHits {
                break
            }
        }

        return hitTargets
    }

    private func renderY(for player: Player) -> Float {
        if player.id == userId && hasJoined {
            return localY
        }
        return smoothedPositions[player.id]?.y ?? player.y
    }

    private func refreshRenderPlayersIfNeeded(now: TimeInterval, force: Bool = false) {
        if !force && !renderPlayersDirty {
            return
        }
        if !force && now - lastRenderOrderUpdateTime < renderOrderUpdateInterval {
            return
        }
        lastRenderOrderUpdateTime = now
        renderPlayersDirty = false
        var decorated: [(y: Float, player: Player)] = []
        decorated.reserveCapacity(players.count)
        for player in players where player.health > 0 {
            decorated.append((y: renderY(for: player), player: player))
        }
        decorated.sort { $0.y < $1.y }
        renderPlayers = decorated.map(\.player)
    }

    private func startWeaponSpawner() {
        weaponSpawnTimer = Timer.scheduledTimer(withTimeInterval: weaponSpawnInterval, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self = self, self.hasJoined, self.isConnected else { return }
                
                // Limit weapons on the ground to avoid clutter.
                if self.weapons.count < maxGroundWeapons {
                    let spawn = self.randomWeaponSpawn()
                    SpawnWeapon.invoke(x: spawn.x, y: spawn.y)
                }
            }
        }
        if let weaponSpawnTimer = weaponSpawnTimer {
            RunLoop.main.add(weaponSpawnTimer, forMode: .common)
        }
    }

    // MARK: - Keyboard (macOS)

    #if canImport(AppKit)
    private var keyDownMonitor: Any?
    private var keyUpMonitor: Any?

    private static let movementKeyCodes: Set<UInt16> = [0, 1, 2, 13, 123, 124, 125, 126]
    private static let menuKeyCodes: Set<UInt16> = [12, 53] // Q=12, Esc=53
    private static let spawnBotKeyCode: UInt16 = 14 // E

    private func shouldConsumeMenuHotkey(_ keyCode: UInt16) -> Bool {
        guard keyCode == 12 else {
            // Esc should always be available for menu toggle.
            return true
        }

        // Don't treat Q as a menu hotkey while typing into text inputs.
        if let responder = NSApp.keyWindow?.firstResponder, responder is NSTextView {
            return false
        }
        return true
    }

    func installKeyboardMonitor() {
        guard keyDownMonitor == nil else { return }

        keyDownMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }

            if Self.menuKeyCodes.contains(event.keyCode) {
                guard self.shouldConsumeMenuHotkey(event.keyCode) else { return event }
                Task { @MainActor in self.isMenuOpen.toggle() }
                return nil
            }

            // While pause/menu UI is open, don't consume gameplay movement keys;
            // let focused controls (e.g. rename TextField) receive raw keyboard input.
            if self.isMenuOpen {
                return event
            }

            if event.keyCode == Self.spawnBotKeyCode {
                Task { @MainActor in
                    guard self.hasJoined, self.isConnected else { return }
                    SoundEffects.shared.play(.buttonPress)
                    SpawnTestPlayer.invoke()
                }
                return nil
            }

            guard Self.movementKeyCodes.contains(event.keyCode) else { return event }
            Task { @MainActor in self.pressedKeys.insert(event.keyCode) }
            return nil
        }
        keyUpMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyUp) { [weak self] event in
            guard let self else { return event }

            if Self.menuKeyCodes.contains(event.keyCode) {
                guard self.shouldConsumeMenuHotkey(event.keyCode) else { return event }
                return nil
            }

            if self.isMenuOpen {
                return event
            }

            if event.keyCode == Self.spawnBotKeyCode {
                return nil
            }

            guard Self.movementKeyCodes.contains(event.keyCode) else { return event }
            Task { @MainActor in self.pressedKeys.remove(event.keyCode) }
            return nil
        }
    }

    private func removeKeyboardMonitors() {
        if let m = keyDownMonitor { NSEvent.removeMonitor(m); keyDownMonitor = nil }
        if let m = keyUpMonitor   { NSEvent.removeMonitor(m); keyUpMonitor = nil }
    }
    #endif

    func uninstallKeyboardMonitor() {
        #if canImport(AppKit)
        removeKeyboardMonitors()
        #endif
    }
}

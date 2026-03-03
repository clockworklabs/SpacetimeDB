import SwiftUI
import Observation
import SpacetimeDB
import Foundation
#if canImport(AppKit)
import AppKit
#endif

#if canImport(AppKit)
private struct ShellCommandResult {
    let exitCode: Int32
    let output: String
}
#endif

@MainActor
@Observable
final class SimpleModuleViewModel: SpacetimeClientDelegate {
    var serverURL: String = "http://127.0.0.1:3000"
    var databaseName: String = "simple-module-demo"
    var connectToken: String = ""
    var draftName: String = ""

    var isConnected: Bool = false
    var isConnecting: Bool = false
    var identityHex: String = "-"
    var tokenPreview: String = "-"
    var statusMessage: String = "Disconnected"
    var isLocalActionRunning: Bool = false
    var localServerReachable: Bool = false
    var modulePublishedLocally: Bool = false
    var modulePublishedOnMaincloud: Bool = false
    var lastConnectBadResponse: Bool = false
    var localSetupLog: String = ""

    var people: [Person] = []

    private var client: SpacetimeClient?
    private var savedToken: String?
    #if canImport(AppKit)
    private var localServerProcess: Process?
    #endif

    init() {
        SpacetimeModule.registerTables()
        #if canImport(AppKit)
        Task { await refreshLocalServerStatus() }
        #endif
    }

    func connect() {
        guard let url = URL(string: serverURL) else {
            statusMessage = "Invalid server URL"
            return
        }

        disconnect(clearStatus: false)
        PersonTable.cache.clear()
        people = []
        statusMessage = "Connecting..."
        isConnecting = true
        lastConnectBadResponse = false

        let client = SpacetimeClient(serverUrl: url, moduleName: databaseName)
        self.client = client
        SpacetimeClient.shared = client
        client.delegate = self
        let manualToken = connectToken.trimmingCharacters(in: .whitespacesAndNewlines)
        let tokenToUse = manualToken.isEmpty ? savedToken : manualToken
        if !manualToken.isEmpty {
            tokenPreview = previewToken(manualToken)
        }
        client.connect(token: tokenToUse)
    }

    func disconnect(clearStatus: Bool = true) {
        isConnecting = false
        isConnected = false
        client?.delegate = nil
        client?.disconnect()
        client = nil
        SpacetimeClient.shared = nil
        if clearStatus {
            statusMessage = "Disconnected"
        }
    }

    func addPerson() {
        let trimmed = draftName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            statusMessage = "Name cannot be empty"
            return
        }
        guard isConnected else {
            statusMessage = "Connect before sending reducers"
            return
        }

        Add.invoke(name: trimmed)
        draftName = ""
    }

    func deletePerson(id: UInt64) {
        guard isConnected else {
            statusMessage = "Connect before sending reducers"
            return
        }
        DeletePerson.invoke(id: id)
        statusMessage = "Sent `delete_person` for row #\(id)"
    }

    func addSamplePerson() {
        guard isConnected else {
            statusMessage = "Connect before sending reducers"
            return
        }
        let sampleNames = ["Alex", "Sam", "Riley", "Jordan", "Taylor", "Casey"]
        let idx = Int(Date().timeIntervalSince1970) % sampleNames.count
        let name = "\(sampleNames[idx]) \(people.count + 1)"
        Add.invoke(name: name)
        statusMessage = "Added sample person: \(name)"
    }

    func clearLocalReplica() {
        PersonTable.cache.clear()
        people = []
    }

    func onConnect() {
        isConnecting = false
        isConnected = true
        if isUsingMaincloud {
            modulePublishedOnMaincloud = true
        } else {
            modulePublishedLocally = true
        }
        lastConnectBadResponse = false
        statusMessage = "Connected"
    }

    func onDisconnect(error: Error?) {
        isConnecting = false
        isConnected = false
        if let error {
            let nsError = error as NSError
            if nsError.domain == NSURLErrorDomain && nsError.code == NSURLErrorBadServerResponse {
                if isUsingMaincloud {
                    modulePublishedOnMaincloud = false
                } else {
                    modulePublishedLocally = false
                }
                lastConnectBadResponse = true
                statusMessage = "Disconnected: bad server response. Publish '\(databaseName)' on this server, then reconnect."
            } else {
                statusMessage = "Disconnected: \(error.localizedDescription)"
            }
        } else {
            statusMessage = "Disconnected"
        }
    }

    func onIdentityReceived(identity: [UInt8], token: String) {
        identityHex = identity.map { String(format: "%02x", $0) }.joined()
        savedToken = token
        tokenPreview = previewToken(token)
    }

    func onTransactionUpdate(message: Data?) {
        people = PersonTable.cache.rows.sorted { lhs, rhs in
            if lhs.createdAtMicros == rhs.createdAtMicros {
                return lhs.id > rhs.id
            }
            return lhs.createdAtMicros > rhs.createdAtMicros
        }
    }

    func onReducerError(reducer: String, message: String, isInternal: Bool) {
        let lowered = message.lowercased()
        if lowered.contains("no such reducer") {
            if isUsingMaincloud {
                modulePublishedOnMaincloud = false
            } else {
                modulePublishedLocally = false
            }
            statusMessage = "Reducer '\(reducer)' is missing on server. Republish '\(databaseName)' then reconnect."
        } else {
            let scope = isInternal ? "internal reducer error" : "reducer error"
            statusMessage = "\(scope) for '\(reducer)': \(message)"
        }
        #if canImport(AppKit)
        appendLocalLog("[Reducer:\(reducer)] \(message)")
        #endif
    }

    func clearLocalSetupLog() {
        localSetupLog = ""
    }

    var localNextStepText: String {
        if !isLocalServerTarget {
            return "Set server URL to localhost or 127.0.0.1 for local flow."
        }
        if isConnected {
            return "Ready: Step 4 add names and watch replicated rows update."
        }
        if !localServerReachable {
            return "Next: Step 1 (Start Local Server)."
        }
        if !modulePublishedLocally || lastConnectBadResponse {
            return "Next: Step 2 (Publish Module)."
        }
        return "Next: Step 3 (Connect)."
    }

    var isUsingMaincloud: Bool {
        guard let host = URLComponents(string: serverURL)?.host?.lowercased() else { return false }
        return host == "maincloud.spacetimedb.com"
    }

    var isLocalServerTarget: Bool {
        guard let host = URLComponents(string: serverURL)?.host?.lowercased() else { return false }
        return host == "127.0.0.1" || host == "localhost"
    }

    var maincloudNextStepText: String {
        if !isUsingMaincloud {
            return "Next: Step 1 (Use Maincloud preset)."
        }
        if isConnected {
            return "Ready: Connected to Maincloud."
        }
        if !modulePublishedOnMaincloud || lastConnectBadResponse {
            return "Next: Step 2 (Publish module to Maincloud)."
        }
        return "Next: Step 3 (Connect)."
    }

    func startLocalServer() {
        #if canImport(AppKit)
        Task { await startLocalServerTask() }
        #else
        statusMessage = "Local server controls are only available on macOS."
        #endif
    }

    func publishLocalModule() {
        #if canImport(AppKit)
        Task { await publishLocalModuleTask() }
        #else
        statusMessage = "Local module publish is only available on macOS."
        #endif
    }

    func bootstrapLocalDemo() {
        #if canImport(AppKit)
        Task { await bootstrapLocalDemoTask() }
        #else
        statusMessage = "Local demo bootstrap is only available on macOS."
        #endif
    }

    func useLocalPreset() {
        serverURL = "http://127.0.0.1:3000"
        lastConnectBadResponse = false
        statusMessage = "Local preset applied."
        #if canImport(AppKit)
        Task { _ = await refreshLocalServerStatus() }
        #endif
    }

    func useMaincloudPreset() {
        serverURL = "https://maincloud.spacetimedb.com"
        lastConnectBadResponse = false
        statusMessage = "Maincloud preset applied."
    }

    func publishMaincloudModule() {
        #if canImport(AppKit)
        Task { await publishMaincloudModuleTask() }
        #else
        statusMessage = "Maincloud module publish is only available on macOS."
        #endif
    }

    func loadCLIToken() {
        #if canImport(AppKit)
        Task { await loadCLITokenTask() }
        #else
        statusMessage = "CLI token loading is only available on macOS."
        #endif
    }

    #if canImport(AppKit)
    private func bootstrapLocalDemoTask() async {
        guard !isLocalActionRunning else {
            statusMessage = "A local setup action is already running."
            return
        }
        isLocalActionRunning = true
        defer { isLocalActionRunning = false }

        appendLocalLog("== Bootstrap local demo ==")
        let started = await ensureLocalServerRunning()
        guard started else { return }
        let published = await publishLocalModuleInternal()
        guard published else { return }
        statusMessage = "Local demo is ready. Connecting..."
        connect()
    }

    private func startLocalServerTask() async {
        guard !isLocalActionRunning else {
            statusMessage = "A local setup action is already running."
            return
        }
        isLocalActionRunning = true
        defer { isLocalActionRunning = false }
        _ = await ensureLocalServerRunning()
    }

    private func publishLocalModuleTask() async {
        guard !isLocalActionRunning else {
            statusMessage = "A local setup action is already running."
            return
        }
        isLocalActionRunning = true
        defer { isLocalActionRunning = false }

        if !(await refreshLocalServerStatus()) {
            statusMessage = "Local server is not reachable. Start it first."
            return
        }
        _ = await publishLocalModuleInternal()
    }

    private func publishMaincloudModuleTask() async {
        guard !isLocalActionRunning else {
            statusMessage = "A setup action is already running."
            return
        }
        isLocalActionRunning = true
        defer { isLocalActionRunning = false }

        let published = await publishModuleInternal(server: "maincloud")
        if published {
            modulePublishedOnMaincloud = true
        }
    }

    private func ensureLocalServerRunning() async -> Bool {
        if await refreshLocalServerStatus() {
            statusMessage = "Local server already running."
            appendLocalLog("Local server already reachable at \(serverURL)")
            return true
        }

        guard localServerProcess?.isRunning != true else {
            statusMessage = "Local server process already started. Waiting for it..."
            appendLocalLog("Local server process is already running.")
            try? await Task.sleep(for: .seconds(1))
            return await refreshLocalServerStatus()
        }

        guard let listenAddress = localListenAddress() else {
            statusMessage = "Server URL must be localhost/127.0.0.1 for local server start."
            appendLocalLog("Refused start: non-local server URL '\(serverURL)'")
            return false
        }

        appendLocalLog("Starting local server on \(listenAddress)...")
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", "spacetime start --non-interactive --listen-addr \(shellQuote(listenAddress))"]

        let outputPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = outputPipe

        outputPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            Task { @MainActor in
                self?.appendLocalLog(text)
            }
        }

        process.terminationHandler = { [weak self] process in
            Task { @MainActor in
                self?.appendLocalLog("Local server exited with code \(process.terminationStatus).")
                self?.localServerProcess = nil
                _ = await self?.refreshLocalServerStatus()
            }
        }

        do {
            try process.run()
            localServerProcess = process
        } catch {
            statusMessage = "Failed to start local server: \(error.localizedDescription)"
            appendLocalLog("Failed to start server: \(error.localizedDescription)")
            return false
        }

        try? await Task.sleep(for: .seconds(1))
        if await refreshLocalServerStatus() {
            statusMessage = "Local server started."
            appendLocalLog("Local server is reachable.")
            return true
        }

        statusMessage = "Started server process, but it is not reachable yet."
        return false
    }

    private func publishLocalModuleInternal() async -> Bool {
        await publishModuleInternal(server: "local")
    }

    private func publishModuleInternal(server: String) async -> Bool {
        let modulePath = localModulePathURL.path
        guard FileManager.default.fileExists(atPath: modulePath) else {
            statusMessage = "Module path not found."
            appendLocalLog("Missing module path: \(modulePath)")
            return false
        }

        appendLocalLog("Publishing module '\(databaseName)' to '\(server)' from \(modulePath)")
        let cmd = "spacetime publish -s \(shellQuote(server)) -p \(shellQuote(modulePath)) \(shellQuote(databaseName)) -c -y"
        let result = await runShellCommand(cmd, currentDirectory: repoRootURL)
        if !result.output.isEmpty {
            appendLocalLog(result.output)
        }

        if result.exitCode == 0 {
            if server == "maincloud" {
                modulePublishedOnMaincloud = true
            } else {
                modulePublishedLocally = true
            }
            lastConnectBadResponse = false
            statusMessage = "Published module '\(databaseName)' to '\(server)'."
            return true
        } else {
            statusMessage = "Module publish failed for '\(server)' (exit \(result.exitCode))."
            return false
        }
    }

    private func loadCLITokenTask() async {
        guard !isLocalActionRunning else {
            statusMessage = "A setup action is already running."
            return
        }
        isLocalActionRunning = true
        defer { isLocalActionRunning = false }

        let result = await runShellCommand("spacetime login show --token", currentDirectory: repoRootURL)
        guard result.exitCode == 0 else {
            if !result.output.isEmpty {
                appendLocalLog(result.output)
            }
            statusMessage = "Failed to read CLI token. Run `spacetime login` in Terminal first."
            return
        }
        guard let token = parseCLIToken(from: result.output) else {
            statusMessage = "Could not parse token from CLI output."
            return
        }
        appendLocalLog("Loaded auth token from local CLI login.")
        connectToken = token
        savedToken = token
        tokenPreview = previewToken(token)
        statusMessage = "Loaded CLI auth token."
    }

    private func refreshLocalServerStatus() async -> Bool {
        guard let pingURL = pingURL() else {
            localServerReachable = false
            return false
        }

        var request = URLRequest(url: pingURL)
        request.timeoutInterval = 1.5
        request.cachePolicy = .reloadIgnoringLocalCacheData

        do {
            let (_, response) = try await URLSession.shared.data(for: request)
            if let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) {
                localServerReachable = true
                return true
            }
        } catch {
            // fallthrough
        }

        localServerReachable = false
        return false
    }

    private func pingURL() -> URL? {
        guard var comps = URLComponents(string: serverURL) else { return nil }
        comps.path = "/v1/ping"
        comps.query = nil
        return comps.url
    }

    private var repoRootURL: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SimpleModuleClient
            .deletingLastPathComponent() // Sources
            .deletingLastPathComponent() // client-swift
            .deletingLastPathComponent() // simple-module
            .deletingLastPathComponent() // demo
            .deletingLastPathComponent() // repo root
    }

    private var localModulePathURL: URL {
        repoRootURL.appendingPathComponent("demo/simple-module/spacetimedb", isDirectory: true)
    }

    private func localListenAddress() -> String? {
        guard let comps = URLComponents(string: serverURL), let host = comps.host else { return nil }
        guard host == "127.0.0.1" || host == "localhost" else { return nil }
        let port = comps.port ?? 3000
        return "\(host):\(port)"
    }

    private func runShellCommand(_ command: String, currentDirectory: URL?) async -> ShellCommandResult {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/bin/zsh")
                process.arguments = ["-lc", command]
                if let currentDirectory {
                    process.currentDirectoryURL = currentDirectory
                }

                let outputPipe = Pipe()
                process.standardOutput = outputPipe
                process.standardError = outputPipe

                do {
                    try process.run()
                } catch {
                    continuation.resume(
                        returning: ShellCommandResult(
                            exitCode: -1,
                            output: "Failed to run command: \(error.localizedDescription)"
                        )
                    )
                    return
                }

                let data = outputPipe.fileHandleForReading.readDataToEndOfFile()
                process.waitUntilExit()
                let output = String(data: data, encoding: .utf8) ?? ""
                continuation.resume(returning: ShellCommandResult(exitCode: process.terminationStatus, output: output))
            }
        }
    }

    private func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    private func appendLocalLog(_ text: String) {
        let normalized = text.replacingOccurrences(of: "\r\n", with: "\n")
        if !localSetupLog.isEmpty && !localSetupLog.hasSuffix("\n") {
            localSetupLog.append("\n")
        }
        localSetupLog.append(normalized.trimmingCharacters(in: .newlines))
        if localSetupLog.count > 24_000 {
            localSetupLog = String(localSetupLog.suffix(24_000))
        }
    }

    private func parseCLIToken(from output: String) -> String? {
        for line in output.split(separator: "\n") {
            let rawLine = String(line)
            guard rawLine.contains("auth token"), let range = rawLine.range(of: " is ") else {
                continue
            }
            let token = rawLine[range.upperBound...].trimmingCharacters(in: .whitespacesAndNewlines)
            if !token.isEmpty {
                return token
            }
        }
        return nil
    }

    #endif

    private func previewToken(_ token: String) -> String {
        if token.count > 16 {
            return "\(token.prefix(8))...\(token.suffix(8))"
        }
        return token
    }
}

private struct SurfaceCard<Content: View>: View {
    let content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    var body: some View {
        content
            .padding(18)
            .background(
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(.ultraThinMaterial)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .stroke(Color.white.opacity(0.24), lineWidth: 1)
            )
    }
}

private struct StatPill: View {
    let title: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.headline.monospaced())
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(
            Capsule(style: .continuous)
                .fill(.regularMaterial)
                .overlay(Capsule(style: .continuous).stroke(Color.white.opacity(0.15), lineWidth: 1))
        )
    }
}

private struct StepActionRow: View {
    let step: Int
    let title: String
    let subtitle: String
    let isComplete: Bool
    let buttonTitle: String
    let buttonRole: ButtonRole?
    let buttonDisabled: Bool
    let action: () -> Void

    init(
        step: Int,
        title: String,
        subtitle: String,
        isComplete: Bool,
        buttonTitle: String,
        buttonRole: ButtonRole? = nil,
        buttonDisabled: Bool = false,
        action: @escaping () -> Void
    ) {
        self.step = step
        self.title = title
        self.subtitle = subtitle
        self.isComplete = isComplete
        self.buttonTitle = buttonTitle
        self.buttonRole = buttonRole
        self.buttonDisabled = buttonDisabled
        self.action = action
    }

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: isComplete ? "checkmark.circle.fill" : "\(step).circle")
                .foregroundStyle(isComplete ? Color.green : Color.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text("Step \(step): \(title)")
                    .font(.subheadline.weight(.semibold))
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(buttonTitle, role: buttonRole, action: action)
                .buttonStyle(.bordered)
                .disabled(buttonDisabled)
        }
    }
}

private struct PersonRow: View {
    let person: Person
    let onDelete: () -> Void

    private var timestampText: String {
        let seconds = TimeInterval(person.createdAtMicros) / 1_000_000
        let date = Date(timeIntervalSince1970: seconds)
        return date.formatted(date: .abbreviated, time: .standard)
    }

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                Circle()
                    .fill(
                        LinearGradient(
                            colors: [Color(red: 0.10, green: 0.59, blue: 0.96), Color(red: 0.18, green: 0.29, blue: 0.79)],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        )
                    )
                    .frame(width: 38, height: 38)
                Text(String(person.name.prefix(1)).uppercased())
                    .font(.headline.weight(.bold))
                    .foregroundStyle(.white)
            }

            VStack(alignment: .leading, spacing: 2) {
                Text(person.name)
                    .font(.body.weight(.semibold))
                Text(timestampText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Text("#\(person.id)")
                .font(.caption.monospaced())
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(Capsule().fill(Color.black.opacity(0.08)))

            Button(role: .destructive, action: onDelete) {
                Image(systemName: "trash")
            }
            .buttonStyle(.bordered)
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color.white.opacity(0.55))
        )
    }
}

struct ContentView: View {
    @State private var vm = SimpleModuleViewModel()
    @State private var showSetupLog = false

    var body: some View {
        ZStack {
            LinearGradient(
                colors: [
                    Color(red: 0.95, green: 0.96, blue: 0.99),
                    Color(red: 0.86, green: 0.90, blue: 0.97)
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            ScrollView {
                VStack(spacing: 14) {
                    header
                    controls
                    replica
                }
                .frame(maxWidth: 1120)
                .frame(maxWidth: .infinity)
                .padding(16)
            }
        }
        .onDisappear {
            vm.disconnect()
        }
        #if canImport(AppKit)
        .onAppear {
            NSApp.activate(ignoringOtherApps: true)
            DispatchQueue.main.async {
                NSApp.windows.first?.makeKeyAndOrderFront(nil)
            }
        }
        #endif
    }

    private var header: some View {
        SurfaceCard {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("Swift Simple Module Client")
                        .font(.title2.weight(.bold))
                    Spacer()
                    Circle()
                        .fill(vm.isConnected ? Color.green : (vm.isConnecting ? Color.orange : Color.red))
                        .frame(width: 10, height: 10)
                }

                Text(vm.statusMessage)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)

                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        StatPill(title: "Rows", value: "\(vm.people.count)")
                        StatPill(title: "Identity", value: vm.identityHex == "-" ? "-" : String(vm.identityHex.prefix(12)))
                        StatPill(title: "Token", value: vm.tokenPreview)
                    }
                }
            }
        }
    }

    private var controls: some View {
        SurfaceCard {
            VStack(spacing: 10) {
                TextField("Server URL", text: $vm.serverURL)
                    .textFieldStyle(.roundedBorder)
                    .autocorrectionDisabled()
                TextField("Database Name", text: $vm.databaseName)
                    .textFieldStyle(.roundedBorder)
                    .autocorrectionDisabled()
                TextField("Auth Token (optional; useful for Maincloud)", text: $vm.connectToken)
                    .textFieldStyle(.roundedBorder)
                    .autocorrectionDisabled()

                HStack(spacing: 8) {
                    Button(vm.isConnected ? "Reconnect" : "Connect") {
                        vm.connect()
                    }
                    .buttonStyle(.borderedProminent)

                    Button("Disconnect") {
                        vm.disconnect()
                    }
                    .buttonStyle(.bordered)
                    .disabled(!vm.isConnected && !vm.isConnecting)

                    Spacer()
                }

                HStack(spacing: 8) {
                    TextField("Name", text: $vm.draftName)
                        .textFieldStyle(.roundedBorder)

                    Button("Add") { vm.addPerson() }
                        .buttonStyle(.borderedProminent)
                        .disabled(!vm.isConnected)

                    Button("Add Sample") { vm.addSamplePerson() }
                        .buttonStyle(.bordered)
                        .disabled(!vm.isConnected)

                    Button("Clear Replica") { vm.clearLocalReplica() }
                        .buttonStyle(.bordered)
                }

                #if canImport(AppKit)
                Divider()

                VStack(alignment: .leading, spacing: 8) {
                    Text("Environment Presets")
                        .font(.subheadline.weight(.semibold))
                    HStack(spacing: 8) {
                        Button("Use Local Preset") { vm.useLocalPreset() }
                            .buttonStyle(.bordered)
                            .disabled(vm.isLocalActionRunning)
                        Button("Use Maincloud Preset") { vm.useMaincloudPreset() }
                            .buttonStyle(.bordered)
                            .disabled(vm.isLocalActionRunning)
                        Spacer()
                    }
                }

                Divider()

                VStack(alignment: .leading, spacing: 8) {
                    Text("Quick Start (Local macOS)")
                        .font(.subheadline.weight(.semibold))

                    HStack(spacing: 8) {
                        Circle()
                            .fill(vm.localServerReachable ? Color.green : Color.orange)
                            .frame(width: 8, height: 8)
                        Text(vm.localNextStepText)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                        Spacer()
                    }

                    StepActionRow(
                        step: 1,
                        title: "Start local server",
                        subtitle: "Launches `spacetime start` for localhost",
                        isComplete: vm.localServerReachable,
                        buttonTitle: "Start Local Server",
                        buttonDisabled: vm.isLocalActionRunning || !vm.isLocalServerTarget
                    ) {
                        vm.startLocalServer()
                    }

                    StepActionRow(
                        step: 2,
                        title: "Publish module (\(vm.databaseName))",
                        subtitle: "Publishes this demo module to your local server",
                        isComplete: (vm.modulePublishedLocally && !vm.lastConnectBadResponse),
                        buttonTitle: "Publish Module",
                        buttonDisabled: vm.isLocalActionRunning || !vm.localServerReachable || !vm.isLocalServerTarget
                    ) {
                        vm.publishLocalModule()
                    }

                    StepActionRow(
                        step: 3,
                        title: "Connect",
                        subtitle: "Connect the app to your local database",
                        isComplete: vm.isConnected && vm.isLocalServerTarget,
                        buttonTitle: vm.isConnected ? "Reconnect" : "Connect",
                        buttonDisabled: vm.isLocalActionRunning
                    ) {
                        vm.connect()
                    }

                    HStack(spacing: 8) {
                        Button("Bootstrap Local (Recommended)") { vm.bootstrapLocalDemo() }
                            .buttonStyle(.borderedProminent)
                            .disabled(vm.isLocalActionRunning || !vm.isLocalServerTarget)
                        Button(showSetupLog ? "Hide Setup Log" : "Show Setup Log") { showSetupLog.toggle() }
                            .buttonStyle(.bordered)
                            .disabled(vm.localSetupLog.isEmpty)
                        Button("Clear Setup Log") { vm.clearLocalSetupLog() }
                            .buttonStyle(.bordered)
                            .disabled(vm.localSetupLog.isEmpty || vm.isLocalActionRunning)
                        Spacer()
                    }
                }
                .padding(12)
                .background(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(Color.white.opacity(0.30))
                )

                VStack(alignment: .leading, spacing: 8) {
                    Text("Quick Test (spacetimedb.com / Maincloud)")
                        .font(.subheadline.weight(.semibold))

                    HStack(spacing: 8) {
                        Circle()
                            .fill(vm.isUsingMaincloud ? Color.green : Color.orange)
                            .frame(width: 8, height: 8)
                        Text(vm.maincloudNextStepText)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                        Spacer()
                    }

                    StepActionRow(
                        step: 1,
                        title: "Switch server URL to Maincloud",
                        subtitle: "Sets `https://maincloud.spacetimedb.com`",
                        isComplete: vm.isUsingMaincloud,
                        buttonTitle: "Use Maincloud Preset",
                        buttonDisabled: vm.isLocalActionRunning
                    ) {
                        vm.useMaincloudPreset()
                    }

                    StepActionRow(
                        step: 2,
                        title: "Publish module (\(vm.databaseName)) to Maincloud",
                        subtitle: "Runs `spacetime publish -s maincloud ...`",
                        isComplete: (vm.modulePublishedOnMaincloud && !vm.lastConnectBadResponse),
                        buttonTitle: "Publish Maincloud Module",
                        buttonDisabled: vm.isLocalActionRunning || !vm.isUsingMaincloud
                    ) {
                        vm.publishMaincloudModule()
                    }

                    StepActionRow(
                        step: 3,
                        title: "Connect to Maincloud",
                        subtitle: "Connect using optional auth token",
                        isComplete: vm.isConnected && vm.isUsingMaincloud,
                        buttonTitle: vm.isConnected ? "Reconnect" : "Connect",
                        buttonDisabled: vm.isLocalActionRunning || !vm.isUsingMaincloud
                    ) {
                        vm.connect()
                    }

                    HStack(spacing: 8) {
                        Text("Optional: load token from `spacetime login show --token`")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Spacer()
                        Button("Load CLI Token") { vm.loadCLIToken() }
                            .buttonStyle(.bordered)
                            .disabled(vm.isLocalActionRunning)
                    }

                    if showSetupLog && !vm.localSetupLog.isEmpty {
                        ScrollView {
                            Text(vm.localSetupLog)
                                .font(.caption.monospaced())
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .textSelection(.enabled)
                                .padding(8)
                        }
                        .frame(minHeight: 72, maxHeight: 120)
                        .background(
                            RoundedRectangle(cornerRadius: 10, style: .continuous)
                                .fill(Color.black.opacity(0.05))
                        )
                    }
                }
                .padding(12)
                .background(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(Color.white.opacity(0.30))
                )
                #endif
            }
        }
    }

    private var replica: some View {
        SurfaceCard {
            VStack(alignment: .leading, spacing: 10) {
                Text("People (Replicated Table: person)")
                    .font(.headline)

                if vm.people.isEmpty {
                    VStack(spacing: 8) {
                        Text("No rows yet")
                            .font(.headline)
                        Text("Connect, then add a person to see real-time replication.")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity, minHeight: 100)
                    .background(
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .fill(Color.white.opacity(0.45))
                    )
                } else {
                    ScrollView {
                        LazyVStack(spacing: 8) {
                            ForEach(vm.people, id: \.id) { person in
                                PersonRow(person: person) {
                                    vm.deletePerson(id: person.id)
                                }
                            }
                        }
                    }
                    .frame(minHeight: 120)
                }
            }
        }
    }
}

// MARK: - macOS lifecycle

#if canImport(AppKit)
@MainActor
private final class SimpleModuleAppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}
#endif

@main
struct SimpleModuleClientApp: App {
    #if canImport(AppKit)
    @NSApplicationDelegateAdaptor(SimpleModuleAppDelegate.self) private var appDelegate
    #endif

    init() {
        #if canImport(AppKit)
        NSApplication.shared.setActivationPolicy(.regular)
        #endif
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
#if os(macOS)
                .frame(minWidth: 760, minHeight: 540)
#endif
        }
    }
}

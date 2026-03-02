import Foundation
import Security

/// Opt-in persistent token storage using the system Keychain.
///
/// Use this to persist SpacetimeDB auth tokens across app launches.
/// Tokens are stored per-module under a service identifier you control.
///
/// Example usage:
/// ```swift
/// let store = KeychainTokenStore(service: "com.myapp.spacetimedb")
///
/// // On launch, load a saved token:
/// let token = store.load(forModule: "my-module")
/// client.connect(token: token)
///
/// // After receiving identity, save the token:
/// func onIdentityReceived(identity: [UInt8], token: String) {
///     store.save(token: token, forModule: "my-module")
/// }
/// ```
public struct KeychainTokenStore: Sendable {
    private let service: String

    /// Creates a Keychain token store.
    /// - Parameter service: The Keychain service identifier, typically your app's bundle ID.
    public init(service: String) {
        self.service = service
    }

    /// Saves a token to the Keychain for the given module name.
    @discardableResult
    public func save(token: String, forModule module: String) -> Bool {
        let data = Data(token.utf8)

        // Delete existing item first to avoid errSecDuplicateItem
        let deleteQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: module,
        ]
        SecItemDelete(deleteQuery as CFDictionary)

        let addQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: module,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]

        let status = SecItemAdd(addQuery as CFDictionary, nil)
        if status != errSecSuccess {
            Log.client.warning("Keychain save failed with status \(status)")
        }
        return status == errSecSuccess
    }

    /// Loads a previously stored token from the Keychain.
    public func load(forModule module: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: module,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess, let data = result as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    /// Deletes a stored token from the Keychain.
    @discardableResult
    public func delete(forModule module: String) -> Bool {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: module,
        ]
        let status = SecItemDelete(query as CFDictionary)
        return status == errSecSuccess || status == errSecItemNotFound
    }
}

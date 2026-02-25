# Kotlin Multiplatform Template

A production-ready Kotlin Multiplatform template with Compose Multiplatform, featuring a full-stack setup with client applications (Android, iOS, Desktop) and a Ktor server with type-safe RPC communication.

## Getting Started

After creating a new repository from this template, rename the project to your desired name:

```bash
./rename-project.sh
```

## Features

- **Multi-platform Support**: Android, iOS, Desktop (JVM), and Server
- **Compose Multiplatform**: Shared UI across all client platforms
- **Clean Architecture**: Separation of Domain, Data, and Presentation layers
- **Type-safe RPC**: Client-server communication using kotlinx-rpc
- **Room Database**: Multiplatform local persistence
- **Dependency Injection**: Koin for DI across all platforms
- **Modern UI**: Material 3 theming with dynamic colors
- **Comprehensive Logging**: Platform-aware logging system
- **TOML Configuration**: App configuration with XDG Base Directory conventions for config and data paths
- **HTTP Client**: Pre-configured Ktor client with logging and JSON serialization
- **NavigationService**: Clean, testable navigation pattern with injectable service

## Project Structure

```
SpacetimedbKotlinSdk/
├── core/                  # Shared foundation (database, logging, networking, config)
├── sharedRpc/             # RPC contracts shared between client & server
├── lib/          # Shared client business logic & UI
├── androidApp/            # Android app entry point
├── desktopApp/            # Desktop (JVM) app entry point
├── server/                # Ktor server application
└── iosApp/                # iOS SwiftUI wrapper
```

## Running the Applications

Each module supports standard Gradle commands: `run`, `build`, `assemble`, etc.

### Android
```bash
./gradlew androidApp:run
```

### Desktop
```bash
./gradlew desktopApp:run
```

Hot reload:
```bash
./gradlew desktopApp:hotRun --auto
```

### iOS
Open `iosApp/iosApp.xcodeproj` in Xcode and run.

### Server
```bash
./gradlew server:run
```
Server runs on `http://localhost:8080`

## Architecture

### Clean Architecture Layers

Each feature follows Clean Architecture with three layers:

- **Domain**: Business logic, models, repository interfaces
- **Data**: Repository implementations, database entities & DAOs, mappers
- **Presentation**: ViewModels (MVI pattern), Compose UI screens

RPC communication uses shared interfaces in `sharedRpc/`, implemented on the server and consumed by clients via generated proxies.

### Compose Screen Pattern

Each screen follows a strict two-layer pattern separating state management from UI:

#### `<Thing>Screen()` — Root Entry Point

Handles ViewModel injection and state collection. Contains no UI logic.

```kotlin
@Composable
fun PersonScreen(
    viewModel: PersonViewModel = koinViewModel<PersonViewModel>(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    PersonContent(
        state = state,
        onAction = viewModel::onAction,
    )
}
```

#### `<Thing>Content()` — Pure UI Composable

Stateless composable that receives everything it needs as parameters. Testable and previewable.

```kotlin
@Composable
fun PersonContent(
    state: PersonState,
    onAction: (PersonAction) -> Unit,
    modifier: Modifier = Modifier,
) {
    // UI implementation
}
```

Content composables can accept optional embedded content for composition:

```kotlin
@Composable
fun HomeContent(
    state: HomeState,
    onAction: (HomeAction) -> Unit,
    modifier: Modifier = Modifier,
    personContent: @Composable (Modifier) -> Unit = { PersonScreen() },
) { ... }
```

#### State, Action, ViewModel

- **State**: `@Immutable` data class with defaults. Uses `ImmutableList` from kotlinx.collections.immutable instead of `List`. Uses `FormField<T>` for form validation.
- **Action**: `@Immutable` sealed interface. Uses `data object` for simple actions, `data class` for parameterized ones. Can nest sub-sealed interfaces for grouping (e.g., `PersonAction.NewPerson`, `PersonAction.LoadedPerson`).
- **ViewModel**: Exposes `StateFlow<State>` and a single `onAction(Action)` function. Uses `viewModelScope` for coroutines. Navigation via injected `NavigationService`.

#### Files per Feature

```
person/
├── domain/
│   ├── model/Person.kt
│   └── repository/PersonRepository.kt
├── data/
│   ├── PersonRepositoryImpl.kt
│   ├── database/PersonDao.kt, PersonEntity.kt, PersonDatabase.kt
│   ├── rpc/PersonRpcClient.kt
│   └── mapper/PersonMapper.kt
└── presentation/
    ├── PersonScreen.kt          # Screen() + Content()
    ├── PersonState.kt           # @Immutable data class
    ├── PersonAction.kt          # @Immutable sealed interface
    ├── PersonViewModel.kt       # ViewModel with StateFlow + onAction
    └── mapper/PersonMapper.kt   # Domain ↔ Form mappers
```

Previews live in a shared `Preview.kt` file using `<Thing>Content()` with mock state.

### Navigation Architecture

This template uses a **NavigationService** pattern for clean, testable, and scalable navigation:

#### NavigationService - Injectable Singleton

```kotlin
class NavigationService {
    fun to(route: Route)                    // Navigate to route
    fun back()                               // Navigate back
    fun toAndClearUpTo(route, clearUpTo)    // Clear back stack
    fun toAndClearAll(route)                 // Reset navigation
}
```

#### ViewModel Usage

ViewModels inject `NavigationService` and use simple API calls:

```kotlin
class HomeViewModel(
    private val nav: NavigationService,  // Injected via Koin
) : ViewModel() {
    fun onAction(action: HomeAction) {
        when (action) {
            HomeAction.OnPersonClicked -> nav.to(Route.Person)
        }
    }
}
```

#### App Setup

Use `NavigationHost` wrapper that auto-observes NavigationService:

```kotlin
@Composable
fun AppReady() {
    NavigationHost(
        navController = rememberNavController(),
        startDestination = Route.Graph,
    ) {
        navigation<Route.Graph>(startDestination = Route.Home) {
            composable<Route.Home> { HomeScreen() }
            composable<Route.Person> { PersonScreen() }
        }
    }
}
```

#### Benefits

- **Simple API**: `nav.to(route)` instead of manual effect management
- **Testable**: Easy to mock NavigationService in unit tests
- **Centralized**: Add analytics, guards, deep links in one place
- **No Boilerplate**: No LaunchedEffect, callbacks, or when expressions
- **True MVI**: Pure unidirectional data flow maintained

### Configuration

App configuration uses TOML files following XDG Base Directory conventions (e.g., `~/.config/spacetimedb_kotlin_sdk/app.toml`). Each `Config` implements `toToml()` to produce commented output, so programmatic saves preserve inline documentation.

#### AppConfigProvider — Runtime Config

`AppConfigProvider` holds the current config as a `StateFlow<AppConfig>`, loaded eagerly at startup via `AppConfigProviderFactory` with Koin's `createdAtStart = true`. Config changes are applied via `updateConfig()`, which persists to disk and triggers downstream service reactions:

- **Logger**: `Log.reconfigure()` applies new log level, format, and file settings immediately
- **RPC Client**: `PersonRpcClient` uses a check-on-use pattern — compares `ServerConnectionConfig` before each call and reconnects if host/port changed

```kotlin
// Example: changing log level at runtime
appConfigProvider.updateConfig { config ->
    config.copy(logging = config.logging.copy(level = LogLevel.ERROR))
}
// Logger is reconfigured, config is persisted to app.toml
```

#### Check-on-Use Pattern

Services that depend on config use a check-on-use pattern: cache the connection and the config snapshot, compare before each call, and recreate if changed. This avoids flow observation and keeps the logic colocated.

```kotlin
class PersonRpcClient(
    private val ktorClient: HttpClient,
    private val appConfigProvider: AppConfigProvider,
) {
    private val mutex = Mutex()
    private var currentServerConfig: ServerConnectionConfig? = null
    private var rpcClientScope: CoroutineScope? = null
    private var rpcClient: RpcClient? = null
    private var peopleService: PeopleService? = null

    private suspend fun service(): PeopleService = mutex.withLock {
        val serverConfig = appConfigProvider.config.value.server
        if (serverConfig != currentServerConfig) {
            rpcClientScope?.cancel()  // closes old connection
            rpcClientScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

            rpcClient = ktorClient.rpc { /* new connection config */ }
            peopleService = rpcClient!!.withService()
            currentServerConfig = serverConfig
        }
        peopleService!!
    }

    suspend fun getAllPeople(): List<PersonRpc> = service().getAllPeople()
}
```

/// Constant storing the application version
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Title used for created windows
pub const WINDOW_TITLE: &str = concat!("Pocket Relay Client v", env!("CARGO_PKG_VERSION"));

/// Window icon bytes
pub const ICON_BYTES: &[u8] = include_bytes!("resources/assets/icon.ico");

/// Name of the file that stores saved pocket relay configuration info
pub const CONFIG_FILE_NAME: &str = "pocket-relay-client.json";

/// The GitHub repository to use for releases
pub const GITHUB_REPOSITORY: &str = "PocketRelay/PocketRelayClientPlugin";

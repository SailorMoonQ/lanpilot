//! Types shared with Dart. Keep them plain (structs and C-like enums) so the
//! generated Dart needs no extra packages.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Bad pairing link, address or key.
    InvalidInput,
    /// No answer in time (offline computer, or a blocked socket; spec 4.4).
    Timeout,
    /// The network refused (no route, socket error).
    Unreachable,
    /// The computer that answered is not the expected one.
    ServerKeyMismatch,
    BadToken,
    WrongPassword,
    /// Too many password attempts; see `retry_after_secs`.
    PasswordLocked,
    PasswordDisabled,
    Denied,
    /// Close code 2: the computer does not know this phone.
    NotPaired,
    /// Close code 3: the computer removed this phone.
    DeviceRemoved,
    /// Close code 4: unpaired at this phone's request.
    Unpaired,
    /// The computer's protocol is older than ours: update the computer.
    ServerTooOld,
    /// The computer's protocol is newer than ours: update the app.
    AppTooOld,
    NotConnected,
    /// The computer answered a request with an error (message has details).
    RequestFailed,
    Closed,
    Internal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeError {
    pub kind: ErrorKind,
    pub message: String,
    pub retry_after_secs: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsKind {
    Unknown,
    Windows,
    Linux,
    Macos,
    Ios,
    Android,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PairedServerInfo {
    pub public_key_hex: String,
    pub short_id: String,
    pub name: String,
    pub os: OsKind,
    /// Candidate addresses as "ip:port".
    pub addrs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredInfo {
    pub short_id: String,
    pub name: String,
    pub os: OsKind,
    pub proto_min: u32,
    pub proto_max: u32,
    /// "ip:port", IPv4 only (the client endpoint binds IPv4).
    pub addrs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionInfo {
    /// Increases with every session; matches `ConnectionEvent::generation`.
    pub generation: u32,
    pub server_name: String,
    pub server_os: OsKind,
    pub version: u32,
    pub capabilities: Vec<String>,
    /// The address that answered, "ip:port".
    pub addr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// Closed by this app (disconnect, replace, reset).
    Local,
    /// The computer ended the session normally.
    ServerClosed,
    TimedOut,
    Lost,
    NotPaired,
    DeviceRemoved,
    Unpaired,
    UnexpectedPeer,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionEvent {
    pub generation: u32,
    pub reason: CloseReason,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtonKind {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    PlayPause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    Mute,
}

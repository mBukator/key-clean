//! Engine errors. Each carries a localization key for a plain-language message and technical
//! details for "View technical details" (§46). None of them contains key data.

use keyclean_core::policy::PolicyError;

/// Something the engine couldn't do. Input is always released when one of these ends a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    /// An engine is already running in this process (only one may own the hook).
    AlreadyRunning,
    /// The engine thread has stopped.
    NotRunning,
    /// A lock was requested while a session is already in progress.
    AlreadyActive,
    /// The lock request was invalid.
    InvalidRequest(PolicyError),
    /// The engine or watchdog thread couldn't be started.
    ThreadSpawn(String),
    /// The engine's hidden window couldn't be created.
    WindowSetup {
        /// The HRESULT.
        code: i32,
        /// Windows' message for it.
        message: String,
    },
    /// Windows refused to install the keyboard hook.
    HookInstall {
        /// The HRESULT.
        code: i32,
        /// Windows' message for it.
        message: String,
    },
    /// The session timer couldn't be started, so the lock was released.
    Timer,
    /// The engine's message loop failed, so the lock was released.
    MessageLoop,
    /// The engine's hidden window was destroyed from outside, so the lock was released and the
    /// engine stopped.
    WindowDestroyed,
    /// Input devices couldn't be listed.
    DeviceQuery {
        /// The Win32 error or HRESULT.
        code: i32,
        /// Windows' message for it.
        message: String,
    },
    /// The engine process (ADR 0009) couldn't be started, didn't answer, or stopped.
    EngineProcess(String),
    /// An error reported by the engine process, carried over the wire as its message key and
    /// technical details.
    Remote {
        /// Key into `locales/<lang>/strings.json`.
        message_key: String,
        /// Technical details.
        details: String,
    },
}

impl EngineError {
    pub(crate) fn window_setup(e: &windows::core::Error) -> Self {
        EngineError::WindowSetup {
            code: e.code().0,
            message: e.message(),
        }
    }

    pub(crate) fn hook_install(e: &windows::core::Error) -> Self {
        EngineError::HookInstall {
            code: e.code().0,
            message: e.message(),
        }
    }

    pub(crate) fn device_query(e: &windows::core::Error) -> Self {
        EngineError::DeviceQuery {
            code: e.code().0,
            message: e.message(),
        }
    }

    /// Key into `locales/<lang>/strings.json` for the plain-language message.
    pub fn message_key(&self) -> &str {
        match self {
            EngineError::AlreadyRunning => "error.engine_already_running",
            EngineError::NotRunning | EngineError::EngineProcess(_) => "error.engine_stopped",
            EngineError::AlreadyActive => "error.already_locked",
            EngineError::InvalidRequest(_) => "error.invalid_duration",
            EngineError::ThreadSpawn(_) | EngineError::WindowSetup { .. } => "error.engine_start",
            EngineError::HookInstall { .. } => "error.hook_install",
            EngineError::Timer | EngineError::MessageLoop | EngineError::WindowDestroyed => {
                "error.engine_failed"
            }
            EngineError::DeviceQuery { .. } => "error.devices",
            EngineError::Remote { message_key, .. } => message_key,
        }
    }

    /// Technical details for "View technical details".
    pub fn details(&self) -> String {
        match self {
            EngineError::AlreadyRunning => "engine already running in this process".into(),
            EngineError::NotRunning => "engine thread is not running".into(),
            EngineError::AlreadyActive => "a session is already in progress".into(),
            EngineError::InvalidRequest(e) => format!("invalid lock request: {e}"),
            EngineError::ThreadSpawn(e) => format!("could not start thread: {e}"),
            EngineError::WindowSetup { code, message } => {
                format!("hidden window setup failed: HRESULT {code:#010X}: {message}")
            }
            EngineError::HookInstall { code, message } => {
                format!("SetWindowsHookExW(WH_KEYBOARD_LL) failed: HRESULT {code:#010X}: {message}")
            }
            EngineError::Timer => "SetTimer failed for the session timer".into(),
            EngineError::MessageLoop => "GetMessageW returned an error".into(),
            EngineError::WindowDestroyed => "the engine's hidden window was destroyed".into(),
            EngineError::DeviceQuery { code, message } => {
                format!("device enumeration failed: {code:#010X}: {message}")
            }
            EngineError::EngineProcess(e) => format!("engine process: {e}"),
            EngineError::Remote { details, .. } => details.clone(),
        }
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.details())
    }
}

impl std::error::Error for EngineError {}

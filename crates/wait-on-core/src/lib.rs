//! Pure Rust wait-on engine. No napi here; `wait-on-napi` binds it for Node.

pub mod socket;
pub mod tcp;

/// Why a resource is not ready yet; `Display` is the reason text shown under `--verbose`.
#[derive(Debug)]
pub enum NotReady {
    TimedOut,
    Io(std::io::Error),
}

impl std::fmt::Display for NotReady {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotReady::TimedOut => f.write_str("timed out"),
            NotReady::Io(e) => e.fmt(f),
        }
    }
}

/// Crate version, surfaced to Node as the addon's `version()`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_workspace_version() {
        assert_eq!(super::version(), "0.1.0");
    }
}

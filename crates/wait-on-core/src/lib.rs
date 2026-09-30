//! Pure Rust wait-on engine. No napi here; `wait-on-napi` binds it for Node.

pub mod http;

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

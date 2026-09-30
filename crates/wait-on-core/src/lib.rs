//! Pure Rust wait-on engine. No napi here; `wait-on-napi` binds it for Node.

pub mod http;
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

/// Size of the file at `path` via a symlink-following stat, or -1 on any error, like
/// `getFileSize` in lib/wait-on.js. Reverse mode and Windows delete-pending rely on -1.
pub fn file_size(path: &str) -> i64 {
    std::fs::metadata(path).map_or(-1, |m| m.len() as i64)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn version_is_workspace_version() {
        assert_eq!(super::version(), "0.1.0");
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("wait-on-core-{}-{name}", std::process::id()))
    }

    #[test]
    fn file_size_is_the_byte_length_of_an_existing_file() {
        let p = temp("exists");
        fs::write(&p, "12345").unwrap();
        let size = super::file_size(p.to_str().unwrap());
        fs::remove_file(&p).unwrap();
        assert_eq!(size, 5);
    }

    #[test]
    fn file_size_is_minus_one_for_a_missing_path() {
        assert_eq!(super::file_size(temp("missing").to_str().unwrap()), -1);
    }

    #[test]
    fn file_size_is_minus_one_for_a_path_under_a_regular_file() {
        let p = temp("notdir");
        fs::write(&p, "x").unwrap();
        let size = super::file_size(p.join("child").to_str().unwrap());
        fs::remove_file(&p).unwrap();
        assert_eq!(size, -1);
    }

    #[test]
    fn file_size_reports_a_directory() {
        assert!(super::file_size(std::env::temp_dir().to_str().unwrap()) >= 0);
    }

    #[cfg(unix)]
    #[test]
    fn file_size_is_minus_one_for_a_dangling_symlink() {
        let p = temp("dangling");
        std::os::unix::fs::symlink(temp("nowhere"), &p).unwrap();
        let size = super::file_size(p.to_str().unwrap());
        fs::remove_file(&p).unwrap();
        assert_eq!(size, -1);
    }

    #[cfg(unix)]
    #[test]
    fn file_size_is_minus_one_when_stat_is_denied() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("locked");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        fs::write(&file, "x").unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
        let denied = fs::metadata(&file).is_err(); // root can still stat: skip then
        let size = super::file_size(file.to_str().unwrap());
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        if denied {
            assert_eq!(size, -1);
        }
    }
}

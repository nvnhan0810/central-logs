use std::path::{Component, Path, PathBuf};

use super::DomainError;

/// An absolute directory on the host that contains log files.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogDirectory(PathBuf);

impl LogDirectory {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let path = Path::new(raw.trim());
        if !path.is_absolute() {
            return Err(DomainError::RelativeDirectory(raw.to_owned()));
        }
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(DomainError::ParentTraversal(raw.to_owned()));
        }
        Ok(Self(path.to_path_buf()))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// A path relative to a [`LogDirectory`]. Empty means the directory itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelativeLogPath(PathBuf);

impl RelativeLogPath {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let trimmed = raw.trim().trim_matches('/');
        let path = Path::new(trimmed);
        let valid = path.components().all(|c| matches!(c, Component::Normal(_)));
        if !valid {
            return Err(DomainError::InvalidLogPath(raw.to_owned()));
        }
        Ok(Self(path.to_path_buf()))
    }

    pub fn root() -> Self {
        Self(PathBuf::new())
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.as_os_str().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_must_be_absolute() {
        assert!(LogDirectory::parse("/var/log/app").is_ok());
        assert_eq!(
            LogDirectory::parse("var/log"),
            Err(DomainError::RelativeDirectory("var/log".into()))
        );
    }

    #[test]
    fn directory_rejects_parent_traversal() {
        assert_eq!(
            LogDirectory::parse("/var/../etc"),
            Err(DomainError::ParentTraversal("/var/../etc".into()))
        );
    }

    #[test]
    fn relative_log_path_accepts_nested_and_root() {
        assert!(RelativeLogPath::parse("").unwrap().is_root());
        assert_eq!(
            RelativeLogPath::parse("/2026/11/app.log/")
                .unwrap()
                .as_path(),
            Path::new("2026/11/app.log")
        );
    }

    #[test]
    fn relative_log_path_rejects_escape_attempts() {
        assert!(RelativeLogPath::parse("../secret").is_err());
        assert!(RelativeLogPath::parse("a/../b").is_err());
        assert!(RelativeLogPath::parse("./a").is_err());
    }
}

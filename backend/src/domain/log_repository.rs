use uuid::Uuid;

use super::{DomainError, LogDirectory, Name, ProjectId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LogRepositoryId(Uuid);

impl LogRepositoryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        Uuid::parse_str(raw)
            .map(Self)
            .map_err(|_| DomainError::InvalidId(raw.to_owned()))
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for LogRepositoryId {
    fn default() -> Self {
        Self::new()
    }
}

/// A named set of host directories that hold the logs of one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRepository {
    id: LogRepositoryId,
    project_id: ProjectId,
    name: Name,
    directories: Vec<LogDirectory>,
}

impl LogRepository {
    pub fn create(
        id: LogRepositoryId,
        project_id: ProjectId,
        name: &str,
        directories: &[&str],
    ) -> Result<Self, DomainError> {
        let name = Name::parse(name)?;
        if directories.is_empty() {
            return Err(DomainError::NoDirectories);
        }
        let mut parsed: Vec<LogDirectory> = Vec::with_capacity(directories.len());
        for raw in directories {
            let dir = LogDirectory::parse(raw)?;
            if parsed.contains(&dir) {
                return Err(DomainError::DuplicateDirectory(raw.trim().to_owned()));
            }
            parsed.push(dir);
        }
        Ok(Self {
            id,
            project_id,
            name,
            directories: parsed,
        })
    }

    pub fn id(&self) -> LogRepositoryId {
        self.id
    }

    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn directories(&self) -> &[LogDirectory] {
        &self.directories
    }

    pub fn contains_directory(&self, directory: &LogDirectory) -> bool {
        self.directories.contains(directory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create(dirs: &[&str]) -> Result<LogRepository, DomainError> {
        LogRepository::create(LogRepositoryId::new(), ProjectId::new(), "api-logs", dirs)
    }

    #[test]
    fn creates_repository_with_directories() {
        let repo = create(&["/var/log/api", "/srv/app/logs"]).unwrap();
        assert_eq!(repo.name().as_str(), "api-logs");
        assert_eq!(repo.directories().len(), 2);
    }

    #[test]
    fn requires_at_least_one_directory() {
        assert_eq!(create(&[]), Err(DomainError::NoDirectories));
    }

    #[test]
    fn rejects_duplicate_directories() {
        assert_eq!(
            create(&["/var/log", "/var/log/"]).err(),
            Some(DomainError::DuplicateDirectory("/var/log/".into()))
        );
    }

    #[test]
    fn propagates_invalid_directory() {
        assert!(matches!(
            create(&["relative/dir"]),
            Err(DomainError::RelativeDirectory(_))
        ));
    }

    #[test]
    fn knows_which_directories_it_owns() {
        let repo = create(&["/var/log/api"]).unwrap();
        let owned = LogDirectory::parse("/var/log/api").unwrap();
        let foreign = LogDirectory::parse("/etc").unwrap();
        assert!(repo.contains_directory(&owned));
        assert!(!repo.contains_directory(&foreign));
    }
}

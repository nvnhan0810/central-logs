use uuid::Uuid;

use super::{DomainError, Name};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectId(Uuid);

impl ProjectId {
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

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

/// A project groups one or more log repositories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    id: ProjectId,
    name: Name,
    description: String,
}

impl Project {
    pub fn create(id: ProjectId, name: &str, description: &str) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            name: Name::parse(name)?,
            description: description.trim().to_owned(),
        })
    }

    pub fn id(&self) -> ProjectId {
        self.id
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_project_with_trimmed_fields() {
        let id = ProjectId::new();
        let project = Project::create(id, " Payments ", "  core services ").unwrap();
        assert_eq!(project.id(), id);
        assert_eq!(project.name().as_str(), "Payments");
        assert_eq!(project.description(), "core services");
    }

    #[test]
    fn rejects_empty_project_name() {
        assert_eq!(
            Project::create(ProjectId::new(), "  ", ""),
            Err(DomainError::EmptyName)
        );
    }

    #[test]
    fn parses_valid_and_invalid_ids() {
        let id = ProjectId::new();
        assert_eq!(ProjectId::parse(&id.as_uuid().to_string()).unwrap(), id);
        assert_eq!(
            ProjectId::parse("nope"),
            Err(DomainError::InvalidId("nope".into()))
        );
    }
}

use super::DomainError;

pub const MAX_NAME_LEN: usize = 100;

/// A human-readable, trimmed, non-empty name of bounded length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name(String);

impl Name {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyName);
        }
        if trimmed.chars().count() > MAX_NAME_LEN {
            return Err(DomainError::NameTooLong { max: MAX_NAME_LEN });
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(Name::parse("  app  ").unwrap().as_str(), "app");
    }

    #[test]
    fn rejects_blank_name() {
        assert_eq!(Name::parse("   "), Err(DomainError::EmptyName));
    }

    #[test]
    fn accepts_name_at_max_length_and_rejects_longer() {
        let max = "a".repeat(MAX_NAME_LEN);
        assert!(Name::parse(&max).is_ok());
        let too_long = "a".repeat(MAX_NAME_LEN + 1);
        assert_eq!(
            Name::parse(&too_long),
            Err(DomainError::NameTooLong { max: MAX_NAME_LEN })
        );
    }
}

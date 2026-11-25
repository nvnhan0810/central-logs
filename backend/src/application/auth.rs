//! Login through the external identity provider (OpenID Connect, authorization code flow with PKCE).
//!
//! The backend keeps identity claims server-side and hands the browser only an opaque session id.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use uuid::Uuid;

use crate::application::error::AppError;
use crate::application::ports::{IdentityProvider, PendingLoginStore, SessionStore};

/// How long a started login may take before the callback is rejected.
pub const LOGIN_TTL: Duration = Duration::from_secs(10 * 60);
/// Lifetime of a session, measured from login. The cookie carries the same limit.
pub const SESSION_TTL: Duration = Duration::from_secs(8 * 60 * 60);
/// Where the user lands after login when the frontend does not ask for a specific page.
pub const DEFAULT_RETURN_TO: &str = "/";
const MAX_RETURN_TO_LEN: usize = 2048;

/// Claims about the signed-in user, as returned by the identity provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

/// A login that has been started but whose callback has not arrived yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingLogin {
    pub code_verifier: String,
    pub return_to: String,
    pub expires_at: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub identity: Identity,
    pub expires_at: SystemTime,
}

/// Result of a successful callback: the session to set in the cookie and where to send the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedLogin {
    pub session_id: String,
    pub return_to: String,
}

pub struct AuthService {
    identity: Arc<dyn IdentityProvider>,
    pending: Arc<dyn PendingLoginStore>,
    sessions: Arc<dyn SessionStore>,
}

impl AuthService {
    pub fn new(
        identity: Arc<dyn IdentityProvider>,
        pending: Arc<dyn PendingLoginStore>,
        sessions: Arc<dyn SessionStore>,
    ) -> Self {
        Self {
            identity,
            pending,
            sessions,
        }
    }

    /// Starts a login and returns the identity provider URL the browser should be sent to.
    pub fn begin_login(&self, return_to: Option<&str>) -> Result<String, AppError> {
        let return_to = validate_return_to(return_to)?;
        let state = random_token();
        let code_verifier = random_token();
        self.pending.save(
            &state,
            PendingLogin {
                code_verifier: code_verifier.clone(),
                return_to,
                expires_at: SystemTime::now() + LOGIN_TTL,
            },
        )?;
        self.identity.authorization_url(&state, &code_verifier)
    }

    /// Finishes a login after the identity provider redirected back with `code` and `state`.
    pub fn complete_login(&self, state: &str, code: &str) -> Result<CompletedLogin, AppError> {
        // `take` consumes the state, so a replayed callback is rejected.
        let pending = self
            .pending
            .take(state)?
            .ok_or_else(|| AppError::Validation("invalid or already used login state".into()))?;
        if pending.expires_at <= SystemTime::now() {
            return Err(AppError::Validation("login expired, please try again".into()));
        }

        let identity = self.identity.exchange_code(code, &pending.code_verifier)?;
        let session_id = random_token();
        self.sessions.save(
            &session_id,
            Session {
                identity,
                expires_at: SystemTime::now() + SESSION_TTL,
            },
        )?;
        Ok(CompletedLogin {
            session_id,
            return_to: pending.return_to,
        })
    }

    /// Returns the signed-in user for a session id, or `None` if it is unknown or expired.
    pub fn current_user(&self, session_id: &str) -> Result<Option<Identity>, AppError> {
        match self.sessions.find(session_id)? {
            Some(session) if session.expires_at > SystemTime::now() => Ok(Some(session.identity)),
            Some(_) => {
                self.sessions.delete(session_id)?;
                Ok(None)
            }
            None => Ok(None),
        }
    }

    pub fn logout(&self, session_id: &str) -> Result<(), AppError> {
        self.sessions.delete(session_id)
    }
}

/// Only same-site relative paths are accepted, so the callback cannot be used as an open redirect.
pub fn validate_return_to(raw: Option<&str>) -> Result<String, AppError> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_RETURN_TO.into());
    };
    let invalid = !raw.starts_with('/')
        || raw.starts_with("//")
        || raw.starts_with("/\\")
        || raw.len() > MAX_RETURN_TO_LEN
        || raw.chars().any(char::is_control);
    if invalid {
        return Err(AppError::Validation(
            "return_to must be a relative path starting with '/'".into(),
        ));
    }
    Ok(raw.to_owned())
}

/// 64 hex characters (two v4 UUIDs, about 244 random bits). Used for state, PKCE verifier and session ids.
fn random_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FakePending(Mutex<HashMap<String, PendingLogin>>);

    impl PendingLoginStore for FakePending {
        fn save(&self, state: &str, login: PendingLogin) -> Result<(), AppError> {
            self.0.lock().unwrap().insert(state.into(), login);
            Ok(())
        }
        fn take(&self, state: &str) -> Result<Option<PendingLogin>, AppError> {
            Ok(self.0.lock().unwrap().remove(state))
        }
    }

    #[derive(Default)]
    struct FakeSessions(Mutex<HashMap<String, Session>>);

    impl SessionStore for FakeSessions {
        fn save(&self, session_id: &str, session: Session) -> Result<(), AppError> {
            self.0.lock().unwrap().insert(session_id.into(), session);
            Ok(())
        }
        fn find(&self, session_id: &str) -> Result<Option<Session>, AppError> {
            Ok(self.0.lock().unwrap().get(session_id).cloned())
        }
        fn delete(&self, session_id: &str) -> Result<(), AppError> {
            self.0.lock().unwrap().remove(session_id);
            Ok(())
        }
    }

    /// Records what the service sent to the provider and answers with a fixed identity.
    #[derive(Default)]
    struct FakeIdp {
        verifiers_seen: Mutex<Vec<(String, String)>>,
        fail_exchange: bool,
    }

    impl IdentityProvider for FakeIdp {
        fn authorization_url(&self, state: &str, code_verifier: &str) -> Result<String, AppError> {
            Ok(format!("https://idp.test/authorize?state={state}&v={code_verifier}"))
        }
        fn exchange_code(&self, code: &str, code_verifier: &str) -> Result<Identity, AppError> {
            self.verifiers_seen
                .lock()
                .unwrap()
                .push((code.into(), code_verifier.into()));
            if self.fail_exchange {
                return Err(AppError::Upstream("token endpoint returned 400".into()));
            }
            Ok(Identity {
                subject: "user-1".into(),
                email: Some("user@example.com".into()),
                name: None,
            })
        }
    }

    fn service(idp: Arc<FakeIdp>) -> (AuthService, Arc<FakePending>, Arc<FakeSessions>) {
        let pending = Arc::new(FakePending::default());
        let sessions = Arc::new(FakeSessions::default());
        let service = AuthService::new(idp, pending.clone(), sessions.clone());
        (service, pending, sessions)
    }

    fn state_from(url: &str) -> String {
        url.split("state=").nth(1).unwrap().split('&').next().unwrap().into()
    }

    #[test]
    fn login_round_trip_creates_a_session_and_consumes_the_state() {
        let idp = Arc::new(FakeIdp::default());
        let (auth, _, _) = service(idp.clone());

        let url = auth.begin_login(Some("/projects/1")).unwrap();
        let state = state_from(&url);
        let done = auth.complete_login(&state, "code-abc").unwrap();

        assert_eq!(done.return_to, "/projects/1");
        let user = auth.current_user(&done.session_id).unwrap().unwrap();
        assert_eq!(user.subject, "user-1");
        assert_eq!(user.email.as_deref(), Some("user@example.com"));

        // The verifier given to the provider at callback is the one embedded in the authorize URL.
        let verifier = url.split("v=").nth(1).unwrap();
        assert_eq!(
            idp.verifiers_seen.lock().unwrap().as_slice(),
            &[("code-abc".to_string(), verifier.to_string())]
        );

        // Replaying the same state is rejected.
        assert!(matches!(
            auth.complete_login(&state, "code-abc"),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn unknown_state_is_rejected_without_calling_the_provider() {
        let idp = Arc::new(FakeIdp::default());
        let (auth, _, _) = service(idp.clone());

        let result = auth.complete_login("forged", "code");

        assert!(matches!(result, Err(AppError::Validation(_))));
        assert!(idp.verifiers_seen.lock().unwrap().is_empty());
    }

    #[test]
    fn provider_failure_does_not_create_a_session() {
        let idp = Arc::new(FakeIdp {
            fail_exchange: true,
            ..Default::default()
        });
        let (auth, _, sessions) = service(idp);
        let state = state_from(&auth.begin_login(None).unwrap());

        let result = auth.complete_login(&state, "bad-code");

        assert!(matches!(result, Err(AppError::Upstream(_))));
        assert!(sessions.0.lock().unwrap().is_empty());
    }

    #[test]
    fn expired_login_is_rejected() {
        let idp = Arc::new(FakeIdp::default());
        let (auth, pending, _) = service(idp.clone());
        pending
            .save(
                "old",
                PendingLogin {
                    code_verifier: "v".into(),
                    return_to: "/".into(),
                    expires_at: SystemTime::UNIX_EPOCH,
                },
            )
            .unwrap();

        assert!(matches!(
            auth.complete_login("old", "code"),
            Err(AppError::Validation(_))
        ));
        assert!(idp.verifiers_seen.lock().unwrap().is_empty());
    }

    #[test]
    fn expired_session_is_treated_as_signed_out_and_removed() {
        let (auth, _, sessions) = service(Arc::new(FakeIdp::default()));
        sessions
            .save(
                "stale",
                Session {
                    identity: Identity {
                        subject: "u".into(),
                        email: None,
                        name: None,
                    },
                    expires_at: SystemTime::UNIX_EPOCH,
                },
            )
            .unwrap();

        assert_eq!(auth.current_user("stale").unwrap(), None);
        assert!(sessions.0.lock().unwrap().is_empty());
    }

    #[test]
    fn logout_removes_the_session() {
        let idp = Arc::new(FakeIdp::default());
        let (auth, _, _) = service(idp);
        let state = state_from(&auth.begin_login(None).unwrap());
        let session = auth.complete_login(&state, "code").unwrap().session_id;

        auth.logout(&session).unwrap();

        assert_eq!(auth.current_user(&session).unwrap(), None);
    }

    #[test]
    fn return_to_must_be_a_same_site_relative_path() {
        assert_eq!(validate_return_to(None).unwrap(), "/");
        assert_eq!(validate_return_to(Some("/a?b=c")).unwrap(), "/a?b=c");
        for bad in [
            "https://evil.test",
            "//evil.test",
            "/\\evil.test",
            "relative",
            "/line\nbreak",
        ] {
            assert!(
                matches!(validate_return_to(Some(bad)), Err(AppError::Validation(_))),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn generated_tokens_are_unique_and_long() {
        let a = random_token();
        let b = random_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}

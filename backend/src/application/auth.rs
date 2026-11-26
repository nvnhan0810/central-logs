//! Login through the external identity provider (OAuth 2.0 authorization code flow with PKCE).
//!
//! The backend keeps the identity provider's refresh token server-side, encrypted, and hands the
//! browser only an opaque session id. When the short-lived access token expires, the backend
//! refreshes it on the next request and rotates the refresh token.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::application::error::AppError;
use crate::application::ports::{IdentityProvider, PendingLoginStore, SecretSealer, SessionStore};

/// How long a started login may take before the callback is rejected.
pub const LOGIN_TTL: Duration = Duration::from_secs(10 * 60);
/// Absolute lifetime of a session, measured from login. The cookie carries the same limit.
pub const SESSION_TTL: Duration = Duration::from_secs(8 * 60 * 60);
/// Upper bound for how long one access token is trusted before a refresh is required.
pub const MAX_ACCESS_TTL: Duration = Duration::from_secs(60 * 60);
/// Where the user lands after login when the frontend does not ask for a specific page.
pub const DEFAULT_RETURN_TO: &str = "/";
const MAX_RETURN_TO_LEN: usize = 2048;
/// Refreshes of different sessions run in parallel; refreshes of one session are serialized.
const REFRESH_LOCK_STRIPES: usize = 64;

/// Claims about the signed-in user, as returned by the identity provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

/// Tokens returned by the identity provider at login or refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityTokens {
    pub identity: Identity,
    /// Lifetime of the access token, as reported by the provider.
    pub expires_in: Duration,
    /// Rotated refresh token. Absent when the provider does not issue one.
    pub refresh_token: Option<String>,
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
    /// Absolute end of the session.
    pub expires_at: SystemTime,
    /// End of the current access token. After this, the next request refreshes it.
    pub access_expires_at: SystemTime,
    /// Refresh token sealed with the session key. `None` when the provider issued none.
    pub refresh_credentials: Option<String>,
}

/// What the browser needs to start a login: the `state` to bind to a cookie, and the provider URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginStart {
    pub state: String,
    pub authorization_url: String,
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
    sealer: Arc<dyn SecretSealer>,
    refresh_locks: Vec<Mutex<()>>,
}

impl AuthService {
    pub fn new(
        identity: Arc<dyn IdentityProvider>,
        pending: Arc<dyn PendingLoginStore>,
        sessions: Arc<dyn SessionStore>,
        sealer: Arc<dyn SecretSealer>,
    ) -> Self {
        Self {
            identity,
            pending,
            sessions,
            sealer,
            refresh_locks: (0..REFRESH_LOCK_STRIPES).map(|_| Mutex::new(())).collect(),
        }
    }

    /// Starts a login. The caller must bind `state` to the browser (cookie) and pass it back to
    /// `complete_login` / `reject_login` together with that cookie value.
    pub fn begin_login(&self, return_to: Option<&str>) -> Result<LoginStart, AppError> {
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
        let authorization_url = self.identity.authorization_url(&state, &code_verifier)?;
        Ok(LoginStart {
            state,
            authorization_url,
        })
    }

    /// Finishes a login after the identity provider redirected back with `code` and `state`.
    /// `cookie` is the `state` value the browser received when the login started.
    pub fn complete_login(
        &self,
        state: &str,
        cookie: Option<&str>,
        code: &str,
    ) -> Result<CompletedLogin, AppError> {
        check_binding(state, cookie)?;
        if code.is_empty() {
            return Err(AppError::Validation("missing authorization code".into()));
        }
        // `take` consumes the state, so a replayed callback is rejected.
        let pending = self
            .pending
            .take(state)?
            .ok_or_else(|| AppError::Validation("invalid or already used login state".into()))?;
        if pending.expires_at <= SystemTime::now() {
            return Err(AppError::Validation(
                "login expired, please try again".into(),
            ));
        }

        let tokens = self.identity.exchange_code(code, &pending.code_verifier)?;
        let session_id = random_token();
        let now = SystemTime::now();
        let refresh_credentials = match tokens.refresh_token.as_deref().filter(|t| !t.is_empty()) {
            Some(refresh) => Some(self.sealer.seal(&session_id, refresh)?),
            None => None,
        };
        self.sessions.save(
            &session_id,
            Session {
                identity: tokens.identity,
                expires_at: now + SESSION_TTL,
                access_expires_at: now + access_ttl(tokens.expires_in),
                refresh_credentials,
            },
        )?;
        Ok(CompletedLogin {
            session_id,
            return_to: pending.return_to,
        })
    }

    /// Abandons a login whose callback reported an error from the provider. Consumes the state so
    /// it cannot be used later.
    pub fn reject_login(&self, state: &str, cookie: Option<&str>) -> Result<(), AppError> {
        check_binding(state, cookie)?;
        self.pending.take(state)?;
        Ok(())
    }

    /// Returns the signed-in user for a session id, or `None` if it is unknown or no longer valid.
    /// An expired access token is refreshed here; the session ends if the provider rejects the
    /// refresh token. A provider outage returns an error and keeps the session.
    pub fn current_user(&self, session_id: &str) -> Result<Option<Identity>, AppError> {
        let Some(session) = self.sessions.find(session_id)? else {
            return Ok(None);
        };
        if session.expires_at <= SystemTime::now() {
            self.sessions.delete(session_id)?;
            return Ok(None);
        }
        if session.access_expires_at > SystemTime::now() {
            return Ok(Some(session.identity));
        }
        self.refresh(session_id)
    }

    pub fn logout(&self, session_id: &str) -> Result<(), AppError> {
        self.sessions.delete(session_id)
    }

    fn refresh(&self, session_id: &str) -> Result<Option<Identity>, AppError> {
        // Holding the gate keeps two requests from using the same refresh token. With rotation,
        // the second use would be rejected by the provider and would sign the user out.
        let _gate = self.refresh_gate(session_id);

        // Read again under the gate: a concurrent request may already have refreshed the session.
        let Some(session) = self.sessions.find(session_id)? else {
            return Ok(None);
        };
        if session.expires_at <= SystemTime::now() {
            self.sessions.delete(session_id)?;
            return Ok(None);
        }
        if session.access_expires_at > SystemTime::now() {
            return Ok(Some(session.identity));
        }

        let Some(sealed) = session.refresh_credentials.as_deref() else {
            self.sessions.delete(session_id)?;
            return Ok(None);
        };
        let Ok(refresh_token) = self.sealer.unseal(session_id, sealed) else {
            self.sessions.delete(session_id)?;
            return Ok(None);
        };

        let tokens = match self.identity.refresh(&refresh_token) {
            Ok(tokens) => tokens,
            Err(AppError::Unauthorized(_)) => {
                self.sessions.delete(session_id)?;
                return Ok(None);
            }
            // Other failures (provider down, bad response) keep the session for a later retry.
            Err(error) => return Err(error),
        };
        if tokens.identity.subject != session.identity.subject {
            self.sessions.delete(session_id)?;
            return Ok(None);
        }
        let rotated = tokens
            .refresh_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                AppError::Upstream("refresh response has no rotated refresh token".into())
            })?;

        let updated = Session {
            identity: tokens.identity,
            expires_at: session.expires_at,
            access_expires_at: SystemTime::now() + access_ttl(tokens.expires_in),
            refresh_credentials: Some(self.sealer.seal(session_id, rotated)?),
        };
        self.sessions.save(session_id, updated.clone())?;
        Ok(Some(updated.identity))
    }

    fn refresh_gate(&self, session_id: &str) -> MutexGuard<'_, ()> {
        let mut hasher = DefaultHasher::new();
        session_id.hash(&mut hasher);
        let index = hasher.finish() as usize % self.refresh_locks.len();
        // A poisoned gate only guards a unit value, so it is safe to keep using it.
        self.refresh_locks[index]
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The browser that started a login must be the one that finishes it. Compared in constant time.
fn check_binding(state: &str, cookie: Option<&str>) -> Result<(), AppError> {
    let matches = !state.is_empty()
        && cookie.is_some_and(|cookie| bool::from(state.as_bytes().ct_eq(cookie.as_bytes())));
    if matches {
        Ok(())
    } else {
        Err(AppError::Validation(
            "login state does not match this browser".into(),
        ))
    }
}

/// The access token is trusted for the provider's lifetime, but never longer than `MAX_ACCESS_TTL`.
fn access_ttl(expires_in: Duration) -> Duration {
    expires_in.min(MAX_ACCESS_TTL)
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

    /// Reversible stand-in for the AES sealer, so tests can read what was stored.
    struct FakeSealer;

    impl SecretSealer for FakeSealer {
        fn seal(&self, session_id: &str, plaintext: &str) -> Result<String, AppError> {
            Ok(format!("sealed:{session_id}:{plaintext}"))
        }
        fn unseal(&self, session_id: &str, sealed: &str) -> Result<String, AppError> {
            sealed
                .strip_prefix(&format!("sealed:{session_id}:"))
                .map(str::to_owned)
                .ok_or_else(|| AppError::Storage("cannot unseal".into()))
        }
    }

    /// Records what the service sent to the provider and answers with fixed tokens.
    #[derive(Default)]
    struct FakeIdp {
        verifiers_seen: Mutex<Vec<(String, String)>>,
        refreshes_seen: Mutex<Vec<String>>,
        fail_exchange: bool,
        /// Refresh outcome: `None` = accept, `Some(error)` = fail with this error.
        refresh_error: Mutex<Option<AppError>>,
        /// Lifetime reported in tokens; tests set it to zero to force an expired access token.
        expires_in: Duration,
        rotated_refresh: Option<String>,
        subject: String,
    }

    impl FakeIdp {
        fn new() -> Self {
            Self {
                expires_in: Duration::from_secs(900),
                rotated_refresh: Some("refresh-2".into()),
                subject: "user-1".into(),
                ..Default::default()
            }
        }

        fn tokens(&self, refresh: Option<String>) -> IdentityTokens {
            IdentityTokens {
                identity: Identity {
                    subject: self.subject.clone(),
                    email: Some("user@example.com".into()),
                    name: None,
                },
                expires_in: self.expires_in,
                refresh_token: refresh,
            }
        }
    }

    impl IdentityProvider for FakeIdp {
        fn authorization_url(&self, state: &str, code_verifier: &str) -> Result<String, AppError> {
            Ok(format!(
                "https://idp.test/authorize?state={state}&v={code_verifier}"
            ))
        }
        fn exchange_code(
            &self,
            code: &str,
            code_verifier: &str,
        ) -> Result<IdentityTokens, AppError> {
            self.verifiers_seen
                .lock()
                .unwrap()
                .push((code.into(), code_verifier.into()));
            if self.fail_exchange {
                return Err(AppError::Upstream("token endpoint returned 400".into()));
            }
            Ok(self.tokens(Some("refresh-1".into())))
        }
        fn refresh(&self, refresh_token: &str) -> Result<IdentityTokens, AppError> {
            self.refreshes_seen
                .lock()
                .unwrap()
                .push(refresh_token.into());
            if let Some(error) = self.refresh_error.lock().unwrap().take() {
                return Err(error);
            }
            Ok(self.tokens(self.rotated_refresh.clone()))
        }
    }

    type Fixture = (AuthService, Arc<FakePending>, Arc<FakeSessions>);

    fn service(idp: Arc<FakeIdp>) -> Fixture {
        let pending = Arc::new(FakePending::default());
        let sessions = Arc::new(FakeSessions::default());
        let service =
            AuthService::new(idp, pending.clone(), sessions.clone(), Arc::new(FakeSealer));
        (service, pending, sessions)
    }

    /// Starts a login and completes it, returning the session id.
    fn sign_in(auth: &AuthService) -> String {
        let start = auth.begin_login(Some("/projects/1")).unwrap();
        auth.complete_login(&start.state, Some(&start.state), "code-abc")
            .unwrap()
            .session_id
    }

    /// Makes the stored access token expired without touching the absolute session lifetime.
    fn expire_access(sessions: &FakeSessions, session_id: &str) {
        let mut map = sessions.0.lock().unwrap();
        map.get_mut(session_id).unwrap().access_expires_at = SystemTime::UNIX_EPOCH;
    }

    #[test]
    fn login_round_trip_creates_a_session_and_consumes_the_state() {
        let idp = Arc::new(FakeIdp::new());
        let (auth, _, sessions) = service(idp.clone());

        let start = auth.begin_login(Some("/projects/1")).unwrap();
        let done = auth
            .complete_login(&start.state, Some(&start.state), "code-abc")
            .unwrap();

        assert_eq!(done.return_to, "/projects/1");
        let user = auth.current_user(&done.session_id).unwrap().unwrap();
        assert_eq!(user.subject, "user-1");
        assert_eq!(user.email.as_deref(), Some("user@example.com"));

        // The refresh token is stored sealed, never in clear.
        let stored = sessions.0.lock().unwrap()[&done.session_id].clone();
        assert_eq!(
            stored.refresh_credentials.as_deref(),
            Some(format!("sealed:{}:refresh-1", done.session_id).as_str())
        );

        // The verifier given to the provider is the one embedded in the authorize URL.
        let verifier = start.authorization_url.split("v=").nth(1).unwrap();
        assert_eq!(
            idp.verifiers_seen.lock().unwrap().as_slice(),
            &[("code-abc".to_string(), verifier.to_string())]
        );

        // Replaying the same state is rejected.
        assert!(matches!(
            auth.complete_login(&start.state, Some(&start.state), "code-abc"),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn callback_from_another_browser_is_rejected_without_calling_the_provider() {
        let idp = Arc::new(FakeIdp::new());
        let (auth, _, _) = service(idp.clone());
        let start = auth.begin_login(None).unwrap();

        // No cookie, and a cookie from a different login, both fail.
        assert!(matches!(
            auth.complete_login(&start.state, None, "code"),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            auth.complete_login(&start.state, Some("other"), "code"),
            Err(AppError::Validation(_))
        ));
        assert!(idp.verifiers_seen.lock().unwrap().is_empty());

        // The real browser can still finish: the failed attempts did not consume the state.
        assert!(
            auth.complete_login(&start.state, Some(&start.state), "code")
                .is_ok()
        );
    }

    #[test]
    fn unknown_state_is_rejected_without_calling_the_provider() {
        let idp = Arc::new(FakeIdp::new());
        let (auth, _, _) = service(idp.clone());

        let result = auth.complete_login("forged", Some("forged"), "code");

        assert!(matches!(result, Err(AppError::Validation(_))));
        assert!(idp.verifiers_seen.lock().unwrap().is_empty());
    }

    #[test]
    fn provider_failure_does_not_create_a_session() {
        let idp = Arc::new(FakeIdp {
            fail_exchange: true,
            ..FakeIdp::new()
        });
        let (auth, _, sessions) = service(idp);
        let start = auth.begin_login(None).unwrap();

        let result = auth.complete_login(&start.state, Some(&start.state), "bad-code");

        assert!(matches!(result, Err(AppError::Upstream(_))));
        assert!(sessions.0.lock().unwrap().is_empty());
    }

    #[test]
    fn provider_error_consumes_the_pending_login() {
        let idp = Arc::new(FakeIdp::new());
        let (auth, pending, _) = service(idp.clone());
        let start = auth.begin_login(None).unwrap();

        auth.reject_login(&start.state, Some(&start.state)).unwrap();

        assert!(pending.0.lock().unwrap().is_empty());
        assert!(matches!(
            auth.complete_login(&start.state, Some(&start.state), "code"),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn expired_login_is_rejected() {
        let idp = Arc::new(FakeIdp::new());
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
            auth.complete_login("old", Some("old"), "code"),
            Err(AppError::Validation(_))
        ));
        assert!(idp.verifiers_seen.lock().unwrap().is_empty());
    }

    #[test]
    fn expired_session_is_treated_as_signed_out_and_removed() {
        let (auth, _, sessions) = service(Arc::new(FakeIdp::new()));
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
                    access_expires_at: SystemTime::UNIX_EPOCH,
                    refresh_credentials: None,
                },
            )
            .unwrap();

        assert_eq!(auth.current_user("stale").unwrap(), None);
        assert!(sessions.0.lock().unwrap().is_empty());
    }

    #[test]
    fn expired_access_token_is_refreshed_and_the_refresh_token_rotated() {
        let idp = Arc::new(FakeIdp::new());
        let (auth, _, sessions) = service(idp.clone());
        let session_id = sign_in(&auth);
        expire_access(&sessions, &session_id);

        let user = auth.current_user(&session_id).unwrap().unwrap();

        assert_eq!(user.subject, "user-1");
        assert_eq!(idp.refreshes_seen.lock().unwrap().as_slice(), ["refresh-1"]);
        // The rotated token replaced the old one, and the access token is valid again.
        let stored = sessions.0.lock().unwrap()[&session_id].clone();
        assert_eq!(
            stored.refresh_credentials.as_deref(),
            Some(format!("sealed:{session_id}:refresh-2").as_str())
        );
        assert!(stored.access_expires_at > SystemTime::now());

        // A valid access token does not call the provider again.
        auth.current_user(&session_id).unwrap().unwrap();
        assert_eq!(idp.refreshes_seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn provider_rejecting_the_refresh_token_signs_the_user_out() {
        let idp = Arc::new(FakeIdp::new());
        let (auth, _, sessions) = service(idp.clone());
        let session_id = sign_in(&auth);
        expire_access(&sessions, &session_id);
        *idp.refresh_error.lock().unwrap() =
            Some(AppError::Unauthorized("refresh token was rejected".into()));

        assert_eq!(auth.current_user(&session_id).unwrap(), None);
        assert!(!sessions.0.lock().unwrap().contains_key(&session_id));
    }

    #[test]
    fn provider_outage_during_refresh_keeps_the_session() {
        let idp = Arc::new(FakeIdp::new());
        let (auth, _, sessions) = service(idp.clone());
        let session_id = sign_in(&auth);
        expire_access(&sessions, &session_id);
        *idp.refresh_error.lock().unwrap() =
            Some(AppError::Upstream("token request failed".into()));

        assert!(matches!(
            auth.current_user(&session_id),
            Err(AppError::Upstream(_))
        ));
        assert!(sessions.0.lock().unwrap().contains_key(&session_id));
    }

    #[test]
    fn refresh_without_a_rotated_token_fails_and_keeps_the_session() {
        let idp = Arc::new(FakeIdp {
            rotated_refresh: None,
            ..FakeIdp::new()
        });
        let (auth, _, sessions) = service(idp);
        let session_id = sign_in(&auth);
        expire_access(&sessions, &session_id);

        assert!(matches!(
            auth.current_user(&session_id),
            Err(AppError::Upstream(_))
        ));
        assert!(sessions.0.lock().unwrap().contains_key(&session_id));
    }

    #[test]
    fn refresh_for_a_different_subject_ends_the_session() {
        // Sign in as user-1, then let the provider answer the refresh as someone else.
        let (auth, _, sessions) = service(Arc::new(FakeIdp::new()));
        let session_id = sign_in(&auth);
        let other_provider = Arc::new(FakeIdp {
            subject: "someone-else".into(),
            ..FakeIdp::new()
        });
        let auth_other = AuthService::new(
            other_provider,
            Arc::new(FakePending::default()),
            sessions.clone(),
            Arc::new(FakeSealer),
        );
        expire_access(&sessions, &session_id);

        assert_eq!(auth_other.current_user(&session_id).unwrap(), None);
        assert!(!sessions.0.lock().unwrap().contains_key(&session_id));
    }

    #[test]
    fn session_without_refresh_token_ends_when_the_access_token_expires() {
        let idp = Arc::new(FakeIdp::new());
        let (auth, _, sessions) = service(idp.clone());
        sessions
            .save(
                "no-refresh",
                Session {
                    identity: Identity {
                        subject: "user-1".into(),
                        email: None,
                        name: None,
                    },
                    expires_at: SystemTime::now() + SESSION_TTL,
                    access_expires_at: SystemTime::UNIX_EPOCH,
                    refresh_credentials: None,
                },
            )
            .unwrap();

        assert_eq!(auth.current_user("no-refresh").unwrap(), None);
        assert!(idp.refreshes_seen.lock().unwrap().is_empty());
    }

    #[test]
    fn logout_removes_the_session() {
        let idp = Arc::new(FakeIdp::new());
        let (auth, _, _) = service(idp);
        let session = sign_in(&auth);

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

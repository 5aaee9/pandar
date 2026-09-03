use std::ffi::c_void;

use anyhow::{Context, ensure};

use super::{
    NoAuthExpected, take_http,
    transaction::{
        AccountView, PluginAccountBytes, PluginAccountMutation, PluginAccountView,
        PluginWithCurrentAccount, capture, transact,
    },
};
use crate::account::{
    persistence, revocation,
    types::{PendingRevocation, PersistedLogin, Profile, SessionInput, SessionKind},
};

const MUTATION_REPLACE: i32 = 1;

pub(super) struct Candidate {
    token: String,
    profile: Profile,
    profile_json: String,
}

#[derive(Clone, Copy)]
pub(super) enum CommitMode {
    Initial,
    Refresh,
}

enum CommitState {
    Pending,
    Applied,
    Rejected,
    Failed(anyhow::Error),
}

struct CommitContext<'a> {
    expected: &'a NoAuthExpected,
    candidate: &'a Candidate,
    mode: CommitMode,
    state: CommitState,
    revoke: Option<(String, PendingRevocation, bool)>,
}

impl Candidate {
    pub(super) fn decode(body: &str) -> anyhow::Result<Self> {
        let session: SessionInput =
            serde_json::from_str(body).context("decode typed no-auth account session")?;
        ensure!(
            !session.token.trim().is_empty(),
            "no-auth session has no token"
        );
        let profile = session.profile.normalize()?;
        let profile_json =
            serde_json::to_string(&profile).context("encode canonical no-auth profile")?;
        Ok(Self {
            token: session.token,
            profile,
            profile_json,
        })
    }

    pub(super) fn token(&self) -> &str {
        &self.token
    }
}

pub(super) fn commit_candidate(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    expected: &NoAuthExpected,
    candidate: &Candidate,
    mode: CommitMode,
) -> anyhow::Result<bool> {
    let mut context = CommitContext {
        expected,
        candidate,
        mode,
        state: CommitState::Pending,
        revoke: None,
    };
    let transaction_result = unsafe {
        transact(
            account_context,
            with_current,
            (&mut context as *mut CommitContext<'_>).cast(),
            commit_transaction,
        )
    };
    if let Some((config_dir, revocation, staged)) = context.revoke.take() {
        revoke_candidate(&config_dir, revocation, staged);
    }
    transaction_result?;
    match context.state {
        CommitState::Applied => Ok(true),
        CommitState::Rejected => Ok(false),
        CommitState::Failed(error) => Err(error),
        CommitState::Pending => anyhow::bail!("account transaction did not commit"),
    }
}

unsafe extern "C" fn commit_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<CommitContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work = (|| {
        let current = unsafe { AccountView::read(view) }?;
        if !commit_current(context.mode, context.expected, &current) {
            stage_candidate(context, &current);
            context.state = CommitState::Rejected;
            return Ok(());
        }
        let login = PersistedLogin {
            hub_url: current.hub_url.clone(),
            token: context.candidate.token.clone(),
            session_kind: SessionKind::NoAuth,
            profile: context.candidate.profile.clone(),
        };
        match persistence::store(&current.config_dir, &login) {
            Ok(durability) => {
                if let Err(error) =
                    durability.require_confirmed("durably persist no-auth Studio login")
                {
                    stage_candidate(context, &current);
                    context.state = CommitState::Failed(error);
                    return Ok(());
                }
            }
            Err(error) => {
                stage_candidate(context, &current);
                context.state = CommitState::Failed(error.context("persist no-auth session"));
                return Ok(());
            }
        }
        let mutation = unsafe { mutation.as_mut() }.context("account mutation is missing")?;
        mutation.action = MUTATION_REPLACE;
        mutation.token = PluginAccountBytes::from_str(&context.candidate.token);
        mutation.user_id = PluginAccountBytes::from_str(&context.candidate.profile.user_id);
        mutation.user_name = PluginAccountBytes::from_str(&context.candidate.profile.user_name);
        mutation.avatar = PluginAccountBytes::from_str(&context.candidate.profile.avatar);
        mutation.profile_json = PluginAccountBytes::from_str(&context.candidate.profile_json);
        mutation.session_kind = SessionKind::NoAuth as i32;
        context.state = CommitState::Applied;
        Ok(())
    })();
    match work {
        Ok(()) => 0,
        Err(error) => {
            context.state = CommitState::Failed(error);
            1
        }
    }
}

fn stage_candidate(context: &mut CommitContext<'_>, current: &AccountView) {
    let revocation = PendingRevocation {
        hub_url: context.expected.hub_url.clone(),
        token: context.candidate.token.clone(),
    };
    let staged = match persistence::enqueue_pending(&current.config_dir, revocation.clone()) {
        Ok(persistence::MutationDurability::Confirmed) => true,
        Ok(persistence::MutationDurability::ChangedUnconfirmed(error)) => {
            eprintln!(
                "pandar no-auth candidate staging failed: change published but durability was not confirmed: {error:#}"
            );
            false
        }
        Err(error) => {
            eprintln!("pandar no-auth candidate staging failed: {error:#}");
            false
        }
    };
    context.revoke = Some((current.config_dir.clone(), revocation, staged));
}

fn revoke_candidate(config_dir: &str, revocation: PendingRevocation, staged: bool) {
    let response = if staged {
        match revocation::revoke(config_dir, revocation) {
            Ok(Some(response)) => Some(take_http(response)),
            Ok(None) => None,
            Err(error) => {
                eprintln!("pandar no-auth candidate revoke failed: {error:#}");
                None
            }
        }
    } else {
        match revocation::revoke_orphan(config_dir, revocation) {
            Ok(Some(response)) => Some(take_http(response)),
            Ok(None) => None,
            Err(error) => {
                eprintln!("pandar no-auth candidate direct revoke failed: {error:#}");
                None
            }
        }
    };
    if let Some(response) = response.filter(|response| response.status != 0) {
        eprintln!(
            "pandar no-auth candidate revoke failed: status={} http_code={} body={}",
            response.status, response.http_code, response.body
        );
    }
}

fn commit_current(mode: CommitMode, expected: &NoAuthExpected, current: &AccountView) -> bool {
    if current.transition_pending
        || current.account_epoch != expected.account_epoch
        || current.config_epoch != expected.config_epoch
        || current.hub_url != expected.hub_url
    {
        return false;
    }
    match mode {
        CommitMode::Initial => current.token.is_empty(),
        CommitMode::Refresh => {
            expected.session_kind == SessionKind::NoAuth as i32
                && current.session_kind == expected.session_kind
                && !expected.token.is_empty()
                && current.token == expected.token
        }
    }
}

pub(super) fn initial_current(
    context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    expected: &NoAuthExpected,
) -> bool {
    unsafe { capture(context, with_current) }
        .is_ok_and(|current| commit_current(CommitMode::Initial, expected, &current))
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::c_void,
        io::{Read, Write},
        net::TcpListener,
    };

    use super::*;
    use crate::account::{
        lifecycle::transaction::{PluginAccountNotification, PluginAccountTransaction},
        persistence::{FaultPoint, fail_next},
        types::{Profile, SessionKind},
    };

    struct TestAccount {
        config_dir: String,
        hub_url: String,
        token: String,
        action: i32,
    }

    unsafe extern "C" fn with_test_account(
        opaque: *mut c_void,
        context: *mut c_void,
        transaction: Option<PluginAccountTransaction>,
    ) -> i32 {
        let account = unsafe { &mut *opaque.cast::<TestAccount>() };
        let empty = PluginAccountBytes::from_str("");
        let view = PluginAccountView {
            config_dir: PluginAccountBytes::from_str(&account.config_dir),
            hub_url: PluginAccountBytes::from_str(&account.hub_url),
            frontend_url: empty,
            token: PluginAccountBytes::from_str(&account.token),
            user_id: empty,
            user_name: empty,
            avatar: empty,
            profile_json: empty,
            account_epoch: 7,
            config_epoch: 11,
            session_kind: SessionKind::NoAuth as i32,
            transition_pending: 0,
        };
        let mut mutation = PluginAccountMutation {
            action: 0,
            notification: PluginAccountNotification::Silent,
            hub_url: empty,
            frontend_url: empty,
            token: empty,
            user_id: empty,
            user_name: empty,
            avatar: empty,
            profile_json: empty,
            session_kind: 0,
            error_body: empty,
            http_code: 0,
        };
        let status = unsafe { transaction.unwrap()(context, &view, &mut mutation) };
        account.action = mutation.action;
        status
    }

    fn profile() -> Profile {
        Profile {
            user_id: "candidate-user".to_owned(),
            user_name: "Candidate User".to_owned(),
            tenant_id: "tenant-1".to_owned(),
            tenant_name: "Tenant".to_owned(),
            avatar: String::new(),
        }
    }

    #[test]
    fn unconfirmed_login_commit_revokes_the_candidate_without_applying_runtime_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let hub_url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.starts_with("delete /api/v1/plugin/session http/1.1"));
            assert!(request.contains("authorization: bearer candidate-token"));
            stream
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().to_string_lossy().into_owned();
        let old_login = PersistedLogin {
            hub_url: hub_url.clone(),
            token: "old-token".to_owned(),
            session_kind: SessionKind::NoAuth,
            profile: profile(),
        };
        persistence::store(&config_dir, &old_login).unwrap();
        let expected = NoAuthExpected {
            hub_url: hub_url.clone(),
            token: "old-token".to_owned(),
            account_epoch: 7,
            config_epoch: 11,
            session_kind: SessionKind::NoAuth as i32,
        };
        let candidate = Candidate {
            token: "candidate-token".to_owned(),
            profile: profile(),
            profile_json: serde_json::to_string(&profile()).unwrap(),
        };
        let mut account = TestAccount {
            config_dir: config_dir.clone(),
            hub_url,
            token: "old-token".to_owned(),
            action: 0,
        };
        fail_next(&[FaultPoint::WritePublish, FaultPoint::WritePublish]);

        let result = commit_candidate(
            (&mut account as *mut TestAccount).cast(),
            Some(with_test_account),
            &expected,
            &candidate,
            CommitMode::Refresh,
        );

        assert!(result.is_err());
        assert_eq!(account.action, 0);
        assert_eq!(persistence::load(&config_dir).unwrap(), None);
        assert!(persistence::load_pending(&config_dir).unwrap().is_empty());
        let completed = std::fs::read_to_string(
            directory
                .path()
                .join("pandar-plugin-completed-revocations.json"),
        )
        .unwrap();
        assert!(!completed.contains("candidate-token"));
        server.join().unwrap();
    }
}

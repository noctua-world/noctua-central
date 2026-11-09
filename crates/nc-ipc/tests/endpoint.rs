//! The listener a daemon serves on holds the catalog's bound of connections,
//! and the handshake runs over the system's own endpoint as it runs over any
//! stream.
//!
//! **A scratch runtime directory and a pipe named for this test alone**: a
//! pipe's name is the whole machine's, so it carries a test prefix, the
//! checkout this was built in, this process and a nonce.

use nc_ipc::{Answer, Request, State, Token, Welcome};

/// A runtime directory for one test, removed however the test leaves.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// A directory of its own, made private as `nc-platform` makes one.
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nc-ipc-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch directory");
        Self(root)
    }

    /// Its endpoint: the socket `s` in it, and a pipe named for this test.
    fn endpoint(&self, name: &str) -> nc_platform::ipc::Endpoint {
        let checkout = env!("CARGO_MANIFEST_DIR")
            .bytes()
            .fold(0x811c_9dc5_u32, |hash, byte| {
                (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
            });
        let mut nonce = [0_u8; 4];
        nc_platform::random(&mut nonce).expect("random bytes");
        let pipe = format!(
            r"\\.\pipe\nc-test-{checkout:08x}-{}-{:08x}-{name}",
            std::process::id(),
            u32::from_le_bytes(nonce)
        );
        nc_platform::ipc::Endpoint::new(&self.0, self.0.join("s"), pipe)
    }
}

/// One connection to `endpoint`, made by a client of `user`'s and accepted by `listener`.
///
/// What each end came to, within a minute: a client that fails before it
/// reaches the listener fails the test rather than hangs it.
async fn connection(
    listener: &mut nc_platform::ipc::Listener,
    endpoint: &nc_platform::ipc::Endpoint,
    user: &nc_platform::User,
) -> (
    Result<nc_platform::ipc::Accepted, nc_platform::ipc::Refused>,
    Result<(nc_platform::ipc::Stream, nc_platform::ipc::Peer), nc_platform::ipc::Refused>,
) {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        tokio::join!(listener.accept(), nc_platform::ipc::connect(endpoint, user))
    })
    .await
    .expect("a connection, accepted and made, within a minute")
}

#[test]
fn the_catalogs_connections_fit_the_pipes_instances() {
    let bound = usize::try_from(nc_catalog::DAEMON.ipc_connections_max).expect("a count");
    assert!(
        (1..=nc_platform::ipc::CONNECTIONS_MAX).contains(&bound),
        "`ipc_connections_max` is {bound}, which a listener cannot be bound to hold"
    );
}

#[tokio::test]
async fn the_daemons_listener_holds_the_catalogs_bound_and_the_handshake_crosses_it() {
    let scratch = Scratch::new("listen");
    let user = nc_platform::user_in(&scratch.0).expect("private");
    let endpoint = scratch.endpoint("listen");
    let instance = nc_platform::hold(&scratch.0, "instance.lock", &user).expect("held");
    let mut listener = nc_ipc::listen(&endpoint, &user, &instance).expect("bound");
    let bound = usize::try_from(nc_catalog::DAEMON.ipc_connections_max).expect("a count");
    let token = Token::fresh().expect("a token");
    let welcome = Welcome {
        pid: std::process::id(),
        instance: [1; 16],
        channel: nc_channel::Channel::Staging,
        state: State::Running,
    };

    // The first connection proves itself and is answered, through the
    // system's endpoint.
    let (accepted, connected) = connection(&mut listener, &endpoint, &user).await;
    let accepted = accepted.expect("accepted");
    let (stream, _) = connected.expect("connected");
    let (served, connected) = tokio::join!(
        nc_ipc::serve(accepted.stream, &token, &welcome),
        nc_ipc::connect(stream, &token)
    );
    let mut served = served.expect("served");
    let (mut client, welcomed) = connected.expect("welcomed");
    assert_eq!(welcomed, welcome);
    let (asked, ()) = tokio::join!(client.ask(&Request::Stop), async {
        assert_eq!(served.next().await.expect("read"), Some(Request::Stop));
        served.answer(&Answer::Stopping).await.expect("answered");
    });
    assert_eq!(asked.expect("answered"), Answer::Stopping);

    // The rest of the bound, each connection held open, then one past it.
    let first = (served, client);
    let mut rest = Vec::new();
    while rest.len() + 1 < bound {
        let (accepted, connected) = connection(&mut listener, &endpoint, &user).await;
        rest.push((
            accepted.expect("within the bound"),
            connected.expect("reached"),
        ));
    }
    let (past, connected) = connection(&mut listener, &endpoint, &user).await;
    match past.map_err(nc_ipc::IpcError::from) {
        Err(refused) => assert_eq!(refused.code(), nc_catalog::ErrorCode::IpcBusy, "{refused}"),
        Ok(accepted) => panic!("a connection past `ipc_connections_max` was {accepted:?}"),
    }
    drop(connected);
    drop((first, rest));
}

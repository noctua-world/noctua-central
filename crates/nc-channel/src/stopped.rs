//! The `stopped` mark: whether a person stopped the channel's daemon on
//! purpose (`NOCTUA.md §17.5`), in the file [`crate::Layout::stopped`] names.
//!
//! **One reader, kernel-free, beside the PID record's**, so every program of
//! an install reads the mark one way: the daemon's restored start, which ends
//! at once while it is there; the CLI's install, which keeps a person's stop;
//! and the desktop interface's alert line (`nc_ipc::alert`), which says the
//! daemon is *stopped* and counts nothing down while it is there
//! (`NOCTUA.md §12.17`). The CLI's
//! `stop` writes it and its `start` removes it; nothing here writes it.
//!
//! **A mark is there only where the system shows it this user's alone**: the
//! state directory holding it is judged first, then the mark. One another
//! account can reach is refused rather than read — read as a stop, it would
//! keep the daemon down for whoever planted it; read as none, it would let a
//! restorer start one a person stopped. A state directory or a mark that is
//! not there is no mark: a profile's first start has neither.

use nc_platform::{Judged, Kind, User};

use crate::Layout;

/// Whether the profile of `layout` carries the `stopped` mark: its state
/// directory and the mark judged `user`'s alone, and no mark where either is
/// not there.
///
/// # Errors
///
/// [`Judged::Refused`] for a state directory or a mark another account can
/// reach, left as it was, and [`Judged::Failed`] when either could not be
/// judged for another reason than not being there.
pub fn on_purpose(layout: &Layout, user: &User) -> Result<bool, Judged> {
    for (path, kind) in [
        (&layout.state, Kind::Directory),
        (&layout.stopped, Kind::File),
    ] {
        match nc_platform::judge(path, kind, user) {
            Ok(()) => {}
            Err(Judged::Failed(absent))
                if absent.error().kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(false);
            }
            Err(judged) => return Err(judged),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nc_platform::Judged;

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("nc-channel-stopped-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        fn user(&self) -> nc_platform::User {
            nc_platform::user_in(&self.0).expect("the scratch is private")
        }

        /// The layout of an install under this scratch's own bases, relocated
        /// by a namespace, as a test's install is.
        fn layout(&self) -> crate::Layout {
            let base = |name: &str| self.0.join(name);
            let install = crate::Install {
                channel: crate::Channel::Staging,
                bases: crate::Bases::new(
                    base("data"),
                    base("config"),
                    base("cache"),
                    base("state"),
                )
                .expect("the scratch's bases are plain absolute paths"),
                namespace: Some(crate::Namespace::parse("t1").expect("a namespace")),
                log_endpoint: None,
            };
            crate::layout(&install, &self.user().to_string())
        }
    }

    /// No state directory and no mark are no stop; a mark private to its user
    /// is one; and a mark, or a state directory, another account can reach is
    /// refused and left as it was, never read as a stop or as none.
    #[test]
    fn a_mark_is_a_stop_only_where_it_is_this_users_alone() {
        let scratch = Scratch::new("read");
        let user = scratch.user();
        let layout = scratch.layout();
        assert_eq!(
            super::on_purpose(&layout, &user).ok(),
            Some(false),
            "a profile with no state directory"
        );
        nc_platform::create_private_directory(&scratch.0.join("state")).expect("the base");
        nc_platform::create_private_directory(&layout.state).expect("the state directory");
        assert_eq!(
            super::on_purpose(&layout, &user).ok(),
            Some(false),
            "a state directory with no mark"
        );
        drop(nc_platform::create_private(&layout.stopped).expect("the mark"));
        assert_eq!(super::on_purpose(&layout, &user).ok(), Some(true));
        nc_platform::testing::open_to_another_user(&layout.stopped).expect("opened");
        assert!(
            matches!(super::on_purpose(&layout, &user), Err(Judged::Refused(_))),
            "a mark another account can reach was read"
        );
        assert!(
            layout.stopped.is_file(),
            "a refused mark was not left as it was"
        );
        nc_platform::testing::make_private(&layout.stopped).expect("private again");
        nc_platform::testing::open_to_another_user(&layout.state).expect("opened");
        assert!(
            matches!(super::on_purpose(&layout, &user), Err(Judged::Refused(_))),
            "a state directory another account can reach was read past"
        );
        nc_platform::testing::make_private(&layout.state).expect("private again");
        std::fs::remove_file(&layout.stopped).expect("the mark is removed");
        assert_eq!(super::on_purpose(&layout, &user).ok(), Some(false));
    }
}

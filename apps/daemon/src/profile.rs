//! The profile a channel keeps: its directories, made private one level at a
//! time or judged so, and the world it decides as (`NOCTUA.md §5.13`, `§17.1`).
//!
//! **One level at a time, never a parent on the way.** Each base directory is
//! the system's, resolved when the install was written, and must be there; the
//! profile's own directory under it, and the log and runtime directories under
//! the state one, are made private from their first moment — a parent made on
//! the way would not be. One that is already there is judged this user's
//! alone, and anything else refuses the start with it left as it was.
//!
//! **The world is a file the installer copies in, and the daemon reads**: the
//! realm and the person every dispatch is decided as, through
//! `nc_authz::LocalWorld::parse`, with the pack the realm selects. A profile
//! without one does not start, and nothing here makes one up — a person with
//! no account has no world yet, which `§5.13` leaves open.

use std::path::{Path, PathBuf};

use nc_authz::{LocalWorld, Pdp};
use nc_channel::Layout;
use nc_platform::{Judged, Kind, User};

use crate::Refused;

/// The directories a profile keeps, in the order they are made: each one's
/// parent is a base or one made before it.
fn directories(layout: &Layout) -> [&Path; 6] {
    [
        &layout.data,
        &layout.config,
        &layout.cache,
        &layout.state,
        &layout.log,
        &layout.runtime,
    ]
}

/// Every directory of the profile, made private where it is missing and judged
/// where it is there.
///
/// # Errors
///
/// [`Refused::NotPrivate`] for one that is not this user's alone, and
/// [`Refused::Unavailable`] for one that could not be made or read — a base
/// that is not there among them.
pub fn prepare(layout: &Layout, user: &User) -> Result<(), Refused> {
    for directory in directories(layout) {
        match nc_platform::create_private_directory(directory) {
            Ok(()) => {}
            // There already, or made by another start between the two: judged
            // either way, never loosened.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                nc_platform::judge(directory, Kind::Directory, user).map_err(Refused::judged)?;
            }
            Err(error) => {
                return Err(Refused::unavailable(
                    format!("`{}` could not be made", directory.display()),
                    error,
                ));
            }
        }
    }
    Ok(())
}

/// The world the profile decides as, read with the pack its realm selects.
///
/// # Errors
///
/// [`Refused::NoWorld`] when the profile holds none, [`Refused::NotPrivate`]
/// for one that is not this user's alone, [`Refused::WorldUnreadable`] for one
/// that does not read, and [`Refused::Unavailable`] when it could not be read.
pub fn world(layout: &Layout, user: &User) -> Result<LocalWorld, Refused> {
    let path = &layout.world;
    match nc_platform::judge(path, Kind::File, user) {
        Ok(()) => {}
        Err(Judged::Failed(failed)) if failed.error().kind() == std::io::ErrorKind::NotFound => {
            return Err(Refused::NoWorld { path: path.clone() });
        }
        Err(judged) => return Err(Refused::judged(judged)),
    }
    let text = std::fs::read_to_string(path).map_err(|error| {
        Refused::unavailable(format!("`{}` could not be read", path.display()), error)
    })?;
    read_world(path, &text)
}

/// `text`, the world file at `path`, read with the pack its realm selects.
fn read_world(path: &Path, text: &str) -> Result<LocalWorld, Refused> {
    let unreadable = |why: String| Refused::WorldUnreadable {
        path: path.to_owned(),
        why,
    };
    let pdp = pack_of(text).map_err(unreadable)?;
    LocalWorld::parse(text, &pdp).map_err(|error| unreadable(error.to_string()))
}

/// The decision point of the pack the world's own realm selects.
///
/// **The realm says which pack it is decided by**, through its `profile`; a
/// world whose realm names a profile this build carries no pack for is
/// refused here, rather than read with another pack's rules.
fn pack_of(text: &str) -> Result<Pdp, String> {
    let value = nc_canon::parse(text).map_err(|error| format!("it is not JSON: {error}"))?;
    let fields = value.into_object().ok_or("it is not one object")?;
    let realm = fields
        .get("realm")
        .and_then(|realm| realm.clone().into_text())
        .ok_or("it names no realm")?;
    let entities = fields
        .get("world")
        .and_then(|world| world.clone().into_array())
        .ok_or("it holds no list of entities")?;
    let profile = entities
        .into_iter()
        .filter_map(nc_canon::Value::into_object)
        .find(|entity| {
            entity
                .get("uid")
                .and_then(|uid| uid.clone().into_object())
                .is_some_and(|uid| {
                    uid.get("type")
                        .and_then(|kind| kind.clone().into_text())
                        .as_deref()
                        == Some("Realm")
                        && uid
                            .get("id")
                            .and_then(|id| id.clone().into_text())
                            .as_deref()
                            == Some(realm.as_str())
                })
        })
        .and_then(|entity| {
            entity
                .get("attrs")
                .and_then(|attrs| attrs.clone().into_object())
        })
        .and_then(|attrs| {
            attrs
                .get("profile")
                .and_then(|profile| profile.clone().into_text())
        })
        .ok_or_else(|| format!("its realm {realm:?} names no profile it is decided by"))?;
    Pdp::embedded(&profile).map_err(|error| error.to_string())
}

/// Copies the world file at `from` into the profile, once it reads, private
/// and whole.
///
/// **Through a file beside it, renamed over it**, so a start never reads half
/// of one: the copy is made private from its first moment, written, synced,
/// and only then put where the world is read from.
///
/// # Errors
///
/// [`Refused::WorldUnreadable`] for a world that does not read,
/// [`Refused::NotPrivate`] for a configuration directory, or a copy left
/// beside the world, that is not this user's alone, and
/// [`Refused::Unavailable`] when it could not be read or written.
pub fn copy_world(from: &Path, layout: &Layout, user: &User) -> Result<(), Refused> {
    use std::io::Write as _;
    let text = std::fs::read_to_string(from).map_err(|error| {
        Refused::unavailable(format!("`{}` could not be read", from.display()), error)
    })?;
    read_world(from, &text)?;
    nc_platform::judge(&layout.config, Kind::Directory, user).map_err(Refused::judged)?;
    let beside = beside_the_world(layout);
    let failed = |doing: &str, error| {
        Refused::unavailable(
            format!("`{}` could not be {doing}", beside.display()),
            error,
        )
    };
    // A copy an interrupted one left behind is removed once it is judged this
    // user's alone; one another account can reach is refused, left as it was.
    match nc_platform::judge(&beside, Kind::File, user) {
        Ok(()) => std::fs::remove_file(&beside).map_err(|error| failed("removed", error))?,
        Err(Judged::Failed(absent)) if absent.error().kind() == std::io::ErrorKind::NotFound => {}
        Err(left) => return Err(Refused::judged(left)),
    }
    let mut file = nc_platform::create_private(&beside).map_err(|error| failed("made", error))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| failed("written", error))?;
    drop(file);
    std::fs::rename(&beside, &layout.world).map_err(|error| failed("renamed", error))?;
    nc_platform::sync_directory(&layout.config).map_err(|error| {
        Refused::unavailable(
            format!("`{}` could not be synced", layout.config.display()),
            error,
        )
    })
}

/// Where a world is written before it is renamed into place.
fn beside_the_world(layout: &Layout) -> PathBuf {
    layout.world.with_extension("json.new")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nc_catalog::ErrorCode;

    use crate::Refused;

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
                .join(format!("nc-daemon-profile-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            for base in ["data", "config", "cache", "state"] {
                nc_platform::create_private_directory(&root.join(base)).expect("a base");
            }
            Self(root)
        }

        /// The layout of a relocated staging install under this scratch.
        fn layout(&self) -> (nc_channel::Layout, nc_platform::User) {
            let user = nc_platform::user_in(&self.0).expect("the scratch is private");
            let install = nc_channel::Install {
                channel: nc_channel::Channel::Staging,
                bases: nc_channel::Bases::new(
                    self.0.join("data"),
                    self.0.join("config"),
                    self.0.join("cache"),
                    self.0.join("state"),
                )
                .expect("plain bases"),
                namespace: Some(nc_channel::Namespace::parse("t1").expect("a namespace")),
                log_endpoint: None,
            };
            (nc_channel::layout(&install, &user.to_string()), user)
        }
    }

    /// The entities every policy case is about, which a development profile's
    /// world is written from.
    const WORLD: &str = include_str!("../../../policy/world.json");

    /// A world file as the installer writes one.
    fn written(realm: &str, person: &str) -> String {
        let mut fields = std::collections::BTreeMap::new();
        fields.insert("realm".to_owned(), nc_canon::Value::Text(realm.to_owned()));
        fields.insert(
            "person".to_owned(),
            nc_canon::Value::Text(person.to_owned()),
        );
        fields.insert(
            "world".to_owned(),
            nc_canon::parse(WORLD).expect("the policy world is JSON"),
        );
        nc_canon::Value::Object(fields).to_canonical()
    }

    /// Every directory of the profile is made private, one level at a time,
    /// and judged the next time.
    #[test]
    fn every_directory_of_the_profile_is_made_private_and_judged_again() {
        let scratch = Scratch::new("made");
        let (layout, user) = scratch.layout();
        super::prepare(&layout, &user).expect("the profile is made");
        // Named from the layout itself, not from the list `prepare` walks, so a
        // directory that list drops is one this finds missing.
        let nc_channel::Layout {
            data,
            config,
            cache,
            state,
            log,
            runtime,
            ..
        } = &layout;
        for directory in [data, config, cache, state, log, runtime] {
            assert!(
                directory.is_dir() && nc_platform::testing::is_private(directory),
                "`{}` was not made private",
                directory.display()
            );
        }
        super::prepare(&layout, &user).expect("the profile is judged again");
        // **And one another account can reach is refused, left as it was.**
        nc_platform::testing::open_to_another_user(log).expect("opened");
        let refusal = super::prepare(&layout, &user).expect_err("a reachable directory");
        assert_eq!(
            refusal.code(),
            ErrorCode::DaemonProfileNotPrivate,
            "{refusal}"
        );
        assert!(!nc_platform::testing::is_private(log));
    }

    /// A base that is not there is not made on the way.
    #[test]
    fn a_missing_base_is_unavailable_and_not_made() {
        let scratch = Scratch::new("nobase");
        let (layout, user) = scratch.layout();
        std::fs::remove_dir(scratch.0.join("cache")).expect("the base is removed");
        let refusal = super::prepare(&layout, &user).expect_err("the base is missing");
        assert_eq!(
            refusal.code(),
            ErrorCode::DaemonProfileUnavailable,
            "{refusal}"
        );
        assert!(
            !scratch.0.join("cache").exists(),
            "a base was made on the way"
        );
    }

    /// A world copied in is the world the profile decides as, and a profile
    /// without one refuses.
    #[test]
    fn a_world_copied_in_is_the_one_the_profile_decides_as() {
        let scratch = Scratch::new("world");
        let (layout, user) = scratch.layout();
        super::prepare(&layout, &user).expect("the profile is made");
        let refusal = super::world(&layout, &user).expect_err("no world yet");
        assert_eq!(refusal.code(), ErrorCode::DaemonNoWorld, "{refusal}");

        let source = scratch.0.join("world.json");
        std::fs::write(&source, written("home", "ana")).expect("a world to copy");
        super::copy_world(&source, &layout, &user).expect("the world is copied");
        assert!(nc_platform::testing::is_private(&layout.world));
        let world = super::world(&layout, &user).expect("the world reads");
        assert_eq!(world.realm().as_str(), "home");
        assert_eq!(world.person().id().as_str(), "ana");
        // Copied again, it replaces the one there, and nothing is left beside it.
        std::fs::write(&source, written("home", "bruno")).expect("another world");
        super::copy_world(&source, &layout, &user).expect("copied again");
        assert_eq!(
            super::world(&layout, &user)
                .expect("the world reads")
                .person()
                .id()
                .as_str(),
            "bruno"
        );
        assert!(!super::beside_the_world(&layout).exists());
    }

    /// A copy left beside the world by an interrupted one is removed, and one
    /// another account can reach is refused and left as it was.
    #[test]
    fn a_copy_left_beside_the_world_is_removed_unless_another_user_can_reach_it() {
        use std::io::Write as _;
        let scratch = Scratch::new("leftover");
        let (layout, user) = scratch.layout();
        super::prepare(&layout, &user).expect("the profile is made");
        let source = scratch.0.join("world.json");
        std::fs::write(&source, written("home", "ana")).expect("a world to copy");
        let beside = super::beside_the_world(&layout);
        let left = b"{\"realm\":\"home\",\"person\":\"an";
        nc_platform::create_private(&beside)
            .and_then(|mut file| file.write_all(left))
            .expect("a copy cut short");
        super::copy_world(&source, &layout, &user).expect("copied over what was left");
        assert_eq!(
            super::world(&layout, &user)
                .expect("the world reads")
                .person()
                .id()
                .as_str(),
            "ana"
        );
        assert!(
            !beside.exists(),
            "what was left beside the world is still there"
        );

        nc_platform::create_private(&beside)
            .and_then(|mut file| file.write_all(left))
            .expect("a copy cut short");
        nc_platform::testing::open_to_another_user(&beside).expect("opened");
        std::fs::write(&source, written("home", "bruno")).expect("another world");
        let refusal =
            super::copy_world(&source, &layout, &user).expect_err("a reachable copy beside it");
        assert_eq!(
            refusal.code(),
            ErrorCode::DaemonProfileNotPrivate,
            "{refusal}"
        );
        assert_eq!(std::fs::read(&beside).expect("reads"), left);
        assert!(!nc_platform::testing::is_private(&beside));
        assert_eq!(
            super::world(&layout, &user)
                .expect("the world reads")
                .person()
                .id()
                .as_str(),
            "ana",
            "a refused copy replaced the world"
        );
    }

    /// A configuration directory another account can reach is refused before
    /// a world is written into it, and nothing is left there.
    #[test]
    fn a_configuration_directory_another_user_can_reach_takes_no_world() {
        let scratch = Scratch::new("config");
        let (layout, user) = scratch.layout();
        super::prepare(&layout, &user).expect("the profile is made");
        let source = scratch.0.join("world.json");
        std::fs::write(&source, written("home", "ana")).expect("a world to copy");
        nc_platform::testing::open_to_another_user(&layout.config).expect("opened");
        let refusal =
            super::copy_world(&source, &layout, &user).expect_err("a reachable directory");
        assert_eq!(
            refusal.code(),
            ErrorCode::DaemonProfileNotPrivate,
            "{refusal}"
        );
        assert!(!layout.world.exists(), "a world was written into it");
        assert!(
            !super::beside_the_world(&layout).exists(),
            "a copy was written into it"
        );
        assert!(!nc_platform::testing::is_private(&layout.config));
    }

    /// A world that does not read is refused before it is copied, and one in
    /// the profile is refused at the start.
    #[test]
    fn a_world_that_does_not_read_is_refused() {
        let scratch = Scratch::new("badworld");
        let (layout, user) = scratch.layout();
        super::prepare(&layout, &user).expect("the profile is made");
        let source = scratch.0.join("world.json");
        std::fs::write(&source, written("home", "mallory")).expect("a world to copy");
        let refusal = super::copy_world(&source, &layout, &user).expect_err("mallory is office's");
        assert_eq!(
            refusal.code(),
            ErrorCode::DaemonWorldUnreadable,
            "{refusal}"
        );
        assert!(!layout.world.exists(), "a refused world was copied in");
        // A realm naming a profile this build carries no pack for.
        let elsewhere = written("office", "mallory");
        std::fs::write(&source, &elsewhere).expect("a world to copy");
        let refusal = super::copy_world(&source, &layout, &user).expect_err("no `org` pack");
        assert!(
            matches!(&refusal, Refused::WorldUnreadable { why, .. } if why.contains("org")),
            "{refusal}"
        );
        // **And one already in the profile**, written there by something other
        // than `copy_world`: the start refuses it with the same code.
        {
            use std::io::Write as _;
            let mut file = nc_platform::create_private(&layout.world).expect("a world file");
            file.write_all(written("home", "mallory").as_bytes())
                .expect("written");
        }
        let refusal = super::world(&layout, &user).expect_err("mallory is office's");
        assert_eq!(
            refusal.code(),
            ErrorCode::DaemonWorldUnreadable,
            "{refusal}"
        );
    }

    /// A world another account can reach is not decided as, and is left as it
    /// was: one it could write is a person it could choose.
    #[test]
    fn a_world_another_user_can_reach_is_refused_and_left_as_it_was() {
        let scratch = Scratch::new("reachable");
        let (layout, user) = scratch.layout();
        super::prepare(&layout, &user).expect("the profile is made");
        let source = scratch.0.join("world.json");
        std::fs::write(&source, written("home", "ana")).expect("a world to copy");
        super::copy_world(&source, &layout, &user).expect("the world is copied");
        nc_platform::testing::open_to_another_user(&layout.world).expect("opened");
        let refusal = super::world(&layout, &user).expect_err("a reachable world");
        assert_eq!(
            refusal.code(),
            ErrorCode::DaemonProfileNotPrivate,
            "{refusal}"
        );
        assert_eq!(
            std::fs::read_to_string(&layout.world).expect("reads"),
            written("home", "ana")
        );
        assert!(!nc_platform::testing::is_private(&layout.world));
    }
}

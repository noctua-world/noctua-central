//! The device a profile mints identifiers on: an Ed25519 seed drawn once, and
//! the flake derived from its public key (`NOCTUA.md §16.2`).
//!
//! **Created by the installer, never by the daemon, and never again.** The seed
//! is 32 bytes from the system's own generator (`nc_platform::random`), written
//! into a file created private from its first moment, which refuses a name
//! already there — a link included — so a seed is never overwritten and a flake
//! never regenerated, the one thing `§16.2` forbids on an existing device.
//! The record beside the seed holds the public key, the candidate the flake was
//! taken from, and the flake, canonically; it is the public half and says nothing a
//! seed could be recovered from.
//!
//! **Read back only as this code writes it.** A seed that is not 32 bytes, a
//! record whose key is not the seed's or whose flake is not that key's
//! candidate, and a record holding any other key are refused, and the daemon
//! does not start on a device it cannot vouch for.
//!
//! **The seed is held no longer than it takes to derive the public key**: it
//! is read, handed to `ed25519_dalek::SigningKey`, which zeroizes it when it is
//! dropped, and the copy read from the file is overwritten. Nothing here logs,
//! prints or sends it.

use std::path::Path;

use nc_channel::Layout;
use nc_ids::{MachineFlake, OpaqueId};
use nc_platform::{Judged, Kind, User};

use crate::Refused;

/// How long a device's seed is: an Ed25519 secret key.
pub const SEED_BYTES: usize = 32;

/// The keys a device's record holds, and no other.
const RECORD_KEYS: [&str; 3] = ["attempt", "flake", "public"];

/// A profile's device: its public key and the flake its identifiers carry.
///
/// **No secret in it**: the seed is read to derive the key
/// and let go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    public: [u8; 32],
    attempt: u32,
    flake: MachineFlake,
}

impl Device {
    /// The flake every identifier this profile mints carries.
    #[must_use]
    pub const fn flake(&self) -> MachineFlake {
        self.flake
    }

    /// The device's Ed25519 public key.
    #[must_use]
    pub const fn public(&self) -> [u8; 32] {
        self.public
    }

    /// The identifier the journal's keeper is named by, derived from the key.
    #[must_use]
    pub fn keeper(&self) -> OpaqueId {
        OpaqueId::of_device_key(&self.public)
    }

    /// The device the profile holds, read and vouched for.
    ///
    /// # Errors
    ///
    /// [`Refused::NoDevice`] when its seed or its record is missing — the
    /// daemon never makes one — [`Refused::DeviceUnreadable`] for one that is
    /// not as this code writes it, [`Refused::NotPrivate`] for one that is not
    /// this user's alone, and [`Refused::Unavailable`] when it could not be read.
    pub fn read(layout: &Layout, user: &User) -> Result<Self, Refused> {
        let public = public_key_of(&layout.device_key, user)?;
        judged_as_there(&layout.device, user)?;
        let text = std::fs::read_to_string(&layout.device).map_err(|error| {
            Refused::unavailable(
                format!("`{}` could not be read", layout.device.display()),
                error,
            )
        })?;
        let unreadable = |why: String| Refused::DeviceUnreadable {
            path: layout.device.clone(),
            why,
        };
        let (attempt, flake, recorded) = record_of(&text).map_err(unreadable)?;
        if recorded != public {
            return Err(unreadable(
                "it records a public key that is not the seed's".to_owned(),
            ));
        }
        let candidate = MachineFlake::candidate(&public, attempt);
        if candidate.value() != flake {
            return Err(unreadable(format!(
                "it records flake {flake}, and the key's candidate {attempt} is {candidate}"
            )));
        }
        Ok(Self {
            public,
            attempt,
            flake: candidate,
        })
    }

    /// The device the profile holds, created first where it has none: a seed
    /// drawn and written, and its record written beside it.
    ///
    /// **What is there is kept**: a seed already there is never drawn again,
    /// and a record already there is read and vouched for; a record missing
    /// beside a seed is written from that seed, so its flake is the one the
    /// seed always derived.
    ///
    /// # Errors
    ///
    /// Every refusal [`Device::read`] answers with, and
    /// [`Refused::Unavailable`] when the seed could not be drawn or a file
    /// could not be written.
    pub fn initialize(layout: &Layout, user: &User) -> Result<Self, Refused> {
        use std::io::Write as _;
        nc_platform::judge(&layout.data, Kind::Directory, user).map_err(Refused::judged)?;
        let failed = |path: &Path, doing: &str, error| {
            Refused::unavailable(format!("`{}` could not be {doing}", path.display()), error)
        };
        let mut seed = [0_u8; SEED_BYTES];
        nc_platform::random(&mut seed)
            .map_err(|error| failed(&layout.device_key, "drawn", error))?;
        let made = match nc_platform::create_private(&layout.device_key) {
            Ok(mut file) => file
                .write_all(&seed)
                .and_then(|()| file.sync_all())
                .map_err(|error| failed(&layout.device_key, "written", error)),
            // A seed already there is the device's, and is never drawn again.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(failed(&layout.device_key, "made", error)),
        };
        // The drawn bytes are the seed only when they were written; either way
        // this copy is done with.
        seed.fill(0);
        core::hint::black_box(&seed);
        made?;
        let public = public_key_of(&layout.device_key, user)?;
        let record = record(&public, 0);
        match nc_platform::create_private(&layout.device) {
            Ok(mut file) => file
                .write_all(record.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(|error| failed(&layout.device, "written", error))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(failed(&layout.device, "made", error)),
        }
        nc_platform::sync_directory(&layout.data)
            .map_err(|error| failed(&layout.data, "synced", error))?;
        Self::read(layout, user)
    }
}

/// The record a device with `public` writes, at candidate `attempt`.
fn record(public: &[u8; 32], attempt: u32) -> String {
    written(
        public,
        attempt,
        MachineFlake::candidate(public, attempt).value(),
    )
}

/// A record holding `public`, `attempt` and `flake`, canonically.
fn written(public: &[u8; 32], attempt: u32, flake: u16) -> String {
    nc_canon::Value::object([
        ("attempt", nc_canon::Value::Integer(i64::from(attempt))),
        ("flake", nc_canon::Value::Integer(i64::from(flake))),
        ("public", nc_canon::Value::text(nc_canon::to_hex(public))),
    ])
    .to_canonical()
}

/// The candidate, the flake and the public key a record says, read only as
/// [`record`] writes one.
fn record_of(text: &str) -> Result<(u32, u16, [u8; 32]), String> {
    let value = nc_canon::parse(text).map_err(|error| format!("it is not JSON: {error}"))?;
    if value.to_canonical() != text {
        return Err("it is not written canonically, as the installer writes it".to_owned());
    }
    let fields = value.into_object().ok_or("it is not one object")?;
    if fields.len() != RECORD_KEYS.len() || !RECORD_KEYS.iter().all(|key| fields.contains_key(*key))
    {
        return Err(format!("it does not hold exactly {RECORD_KEYS:?}"));
    }
    let number = |key: &str| match fields.get(key) {
        Some(nc_canon::Value::Integer(number)) => Ok(*number),
        _ => Err(format!("its {key:?} is not a number")),
    };
    let attempt = u32::try_from(number("attempt")?).map_err(|_| "its attempt is out of range")?;
    let flake = u16::try_from(number("flake")?).map_err(|_| "its flake is out of range")?;
    let public = match fields.get("public") {
        Some(nc_canon::Value::Text(hex)) => nc_canon::from_hex(hex)
            .ok()
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            // Written as `record` writes it, which is lower case.
            .filter(|public| nc_canon::to_hex(public) == *hex)
            .ok_or("its public key is not 32 bytes of lower-case hexadecimal")?,
        _ => return Err("its public key is not text".to_owned()),
    };
    Ok((attempt, flake, public))
}

/// The public key of the seed at `path`, once the file is judged and read.
fn public_key_of(path: &Path, user: &User) -> Result<[u8; 32], Refused> {
    judged_as_there(path, user)?;
    let mut bytes = std::fs::read(path).map_err(|error| {
        Refused::unavailable(format!("`{}` could not be read", path.display()), error)
    })?;
    let Ok(mut seed) = <[u8; SEED_BYTES]>::try_from(bytes.as_slice()) else {
        let length = bytes.len();
        bytes.fill(0);
        return Err(Refused::DeviceUnreadable {
            path: path.to_owned(),
            why: format!("a seed is {SEED_BYTES} bytes, and this one is {length}"),
        });
    };
    bytes.fill(0);
    core::hint::black_box(&bytes);
    let public = ed25519_dalek::SigningKey::from_bytes(&seed)
        .verifying_key()
        .to_bytes();
    seed.fill(0);
    core::hint::black_box(&seed);
    Ok(public)
}

/// A file of the device judged this user's alone, or refused as missing.
fn judged_as_there(path: &Path, user: &User) -> Result<(), Refused> {
    match nc_platform::judge(path, Kind::File, user) {
        Ok(()) => Ok(()),
        Err(Judged::Failed(failed)) if failed.error().kind() == std::io::ErrorKind::NotFound => {
            Err(Refused::NoDevice {
                path: path.to_owned(),
            })
        }
        Err(judged) => Err(Refused::judged(judged)),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nc_catalog::ErrorCode;
    use nc_ids::MachineFlake;

    use super::Device;

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        /// A private directory of its own, holding the four bases.
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("nc-daemon-device-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            for base in ["data", "config", "cache", "state"] {
                nc_platform::create_private_directory(&root.join(base)).expect("a base");
            }
            Self(root)
        }

        /// A relocated staging install's prepared profile under the scratch.
        fn profile(&self) -> (nc_channel::Layout, nc_platform::User) {
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
            let layout = nc_channel::layout(&install, &user.to_string());
            crate::profile::prepare(&layout, &user).expect("the profile is made");
            (layout, user)
        }
    }

    /// The installer draws a seed, private, of a seed's length, and records
    /// the flake as the public key's first candidate.
    #[test]
    fn a_device_is_a_seed_drawn_once_and_the_flake_of_its_key() {
        let held = Scratch::new("made");
        let (layout, user) = held.profile();
        let device = Device::initialize(&layout, &user).expect("the device is made");
        let seed = std::fs::read(&layout.device_key).expect("the seed reads");
        assert_eq!(seed.len(), super::SEED_BYTES);
        assert!(nc_platform::testing::is_private(&layout.device_key));
        assert!(nc_platform::testing::is_private(&layout.device));
        let public = ed25519_dalek::SigningKey::from_bytes(
            &<[u8; 32]>::try_from(seed.as_slice()).expect("32 bytes"),
        )
        .verifying_key()
        .to_bytes();
        assert_eq!(device.public(), public);
        assert_eq!(device.flake(), MachineFlake::candidate(&public, 0));
        assert_eq!(device.keeper(), nc_ids::OpaqueId::of_device_key(&public));
        assert_eq!(
            Device::read(&layout, &user).expect("the device reads"),
            device
        );
    }

    /// The seed is never drawn again: a second initialize keeps it, and a
    /// record lost beside it is written from it, to the same flake.
    #[test]
    fn the_seed_is_never_regenerated() {
        let held = Scratch::new("kept");
        let (layout, user) = held.profile();
        let first = Device::initialize(&layout, &user).expect("the device is made");
        let seed = std::fs::read(&layout.device_key).expect("the seed reads");
        let again = Device::initialize(&layout, &user).expect("initialized again");
        assert_eq!(again, first);
        assert_eq!(std::fs::read(&layout.device_key).expect("reads"), seed);
        std::fs::remove_file(&layout.device).expect("the record is removed");
        let rewritten = Device::initialize(&layout, &user).expect("the record is written again");
        assert_eq!(rewritten, first, "a lost record moved the flake");
        assert_eq!(std::fs::read(&layout.device_key).expect("reads"), seed);
    }

    /// The daemon never makes a device: a profile without one refuses.
    #[test]
    fn a_profile_without_a_device_refuses_and_makes_none() {
        let held = Scratch::new("none");
        let (layout, user) = held.profile();
        let refusal = Device::read(&layout, &user).expect_err("no device yet");
        assert_eq!(refusal.code(), ErrorCode::DaemonNoDevice, "{refusal}");
        assert!(!layout.device_key.exists(), "reading made a seed");
    }

    /// A seed of another length, and a record that is not the seed's, are not
    /// a device this code vouches for.
    #[test]
    fn a_device_not_as_this_code_writes_it_is_refused() {
        let held = Scratch::new("bad");
        let (layout, user) = held.profile();
        let device = Device::initialize(&layout, &user).expect("the device is made");
        let record = std::fs::read_to_string(&layout.device).expect("reads");
        for (what, text) in [
            (
                "another flake",
                record.replacen("\"flake\":", "\"flake\":1", 1),
            ),
            ("another key", super::record(&[7; 32], 0)),
            // The flake the seed's, so only the key tells them apart.
            (
                "another key, with the seed's flake",
                super::written(&[7; 32], 0, device.flake().value()),
            ),
            ("a key too many", record.replacen('{', "{\"a\":1,", 1)),
            ("not canonical", format!(" {record}")),
        ] {
            std::fs::write(&layout.device, &text).expect("rewritten");
            let refusal = Device::read(&layout, &user).expect_err(what);
            assert_eq!(
                refusal.code(),
                ErrorCode::DaemonDeviceUnreadable,
                "{what}: {refusal}"
            );
        }
        std::fs::write(&layout.device, &record).expect("restored");
        std::fs::write(&layout.device_key, [1_u8; 31]).expect("a short seed");
        let refusal = Device::read(&layout, &user).expect_err("a short seed");
        assert_eq!(
            refusal.code(),
            ErrorCode::DaemonDeviceUnreadable,
            "{refusal}"
        );
    }
}

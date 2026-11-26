//! The channel's settings: `settings.mcp.read` and `settings.mcp.set`
//! (`NOCTUA.md §12.15`).
//!
//! **A setting is two commands of the kernel's own, and its value is the
//! kernel's entry in the channel's store.** `settings.<setting>.read` answers
//! the setting as the one object `settings.<setting>.set` takes whole. Both are
//! decided on the platform's feature of the setting's name — `Read` and `Write`
//! on `Feature::"mcp"`, which the family pack gives every person of the realm
//! (`family.a_member_switches_mcp`) and no agent or device — and the settings
//! are listed under category `noctua.settings`, beside a person's agents'
//! commands (`crate::agents`), which change who may act for them too, so there
//! is no second list of them. The value is one entry of the local store under the holder
//! [`HOLDER`], which no plugin's identity can be (`crate::manifest`), and a
//! setting never written is its default: MCP is off until a person turns it
//! on. The store is the channel's profile's own, so a setting switched in one
//! channel is switched in no other (`NOCTUA.md §17.1`).
//!
//! **The set is a step the person's undo takes back.** It stages the value in
//! the unit the decision point opened for it, which commits whole with its
//! journal entry or not at all (`crate::units`), and its one effect is that
//! entry — so it is no step that left the machine
//! ([`writes_only_its_entry`], `crate::history`), and an undo puts the value
//! back as it was and reaches the steps before it. **It runs only at the top of
//! a call tree** (`crate::registry::only_at_the_top`): dispatched from inside a
//! command — a plugin run as the person, a timer's or an event's callback — it
//! is refused with `[NC-KERN-0025]` before it is decided. The read runs
//! anywhere, since it changes nothing.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! Who may switch whether agents are let in: a person, by a decided and
//! journaled dispatch of their own, never a plugin, a timer or an event in
//! their name, and never an agent or a device, which the permit refuses. A
//! value this module did not write — any text in the entry but the two it
//! writes — is refused as a row the kernel did not write (`internal`), never
//! read as on or as off. **What it does not answer:** a process running as the
//! same user, which can run the CLI as the person or edit the entry in the
//! profile's database, as it can any of the person's data; and what MCP being
//! on lets in, which is the daemon's to enforce and the decision point's to
//! decide per call.

use std::sync::{Arc, Mutex};

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_ids::OpaqueId;
use nc_local::{Entry, LocalError};

use crate::backend::StoreBackend;
use crate::dispatch::{self, CallTree, Failure, Refusal, RefusalKind};
use crate::{Environment, Shared, units};

/// The command that reads whether MCP is on.
pub(crate) const MCP_READ: &str = "settings.mcp.read";

/// The command that switches MCP on or off.
pub(crate) const MCP_SET: &str = "settings.mcp.set";

/// Whether MCP is on: the one field of MCP's setting.
const ON: &str = "on";

/// The holder the kernel keeps its settings under in the store: the store's
/// own name for the kernel's holder.
///
/// Never a plugin's identity, which is `<word>:<word>`, so no plugin reads or
/// writes it.
pub(crate) const HOLDER: &str = nc_local::KERNEL;

/// The key MCP's setting is kept under.
const MCP: &str = "settings.mcp";

/// Whether MCP is on before a person writes it: off.
const MCP_DEFAULT: bool = false;

/// How many characters of an entry no writer here wrote a refusal repeats.
const QUOTED: usize = 64;

/// The channel's settings, each one never written its default.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Whether MCP is on, which a person switches with `settings.mcp.set`.
    pub mcp: bool,
}

/// What MCP's setting is written as, and answered as: `{"on":<bool>}`.
fn written(on: bool) -> String {
    Value::object([(ON, Value::Bool(on))]).to_canonical()
}

/// Whether a command's one effect is its setting's entry, in its own unit.
///
/// The history takes such a command for no step that left the machine, so
/// the person's undo puts the value back and reaches past it. It names the
/// command itself and never the namespace: a later setting whose effect does
/// leave the machine is not taken back by being a setting.
pub(crate) fn writes_only_its_entry(name: &str) -> bool {
    name == MCP_SET
}

/// The settings `store` holds committed for `realm`.
///
/// # Errors
///
/// What the store refused the read with, as `crate::units::store_failed`
/// answers it; and `internal`, with the code of a row the kernel did not
/// write, for an entry holding anything but what this module writes.
pub(crate) async fn read<S: StoreBackend>(
    store: &S,
    realm: &OpaqueId,
) -> Result<Settings, Refusal> {
    let entry = store
        .get(realm, HOLDER, MCP)
        .await
        .map_err(|failed| units::store_failed(&failed))?;
    Ok(Settings {
        mcp: on_in(entry.as_ref())?,
    })
}

/// Whether `entry`, MCP's committed entry, says MCP is on.
///
/// None is the default. **Only the two texts this module writes are read**,
/// whatever else the store keeps: JSON of another shape, `on` of another type,
/// or a field beside it is a row the kernel did not write.
fn on_in(entry: Option<&Entry>) -> Result<bool, Refusal> {
    let Some(entry) = entry else {
        return Ok(MCP_DEFAULT);
    };
    [true, false]
        .into_iter()
        .find(|on| entry.value == written(*on))
        .ok_or_else(|| {
            units::store_failed(&LocalError::Unexpected(format!(
                "the kernel's entry `{MCP}` holds {}, which nothing here writes, so it was \
                 read as neither on nor off",
                crate::registry::quoted_up_to(&entry.value, QUOTED)
            )))
        })
}

/// `settings.mcp.read`: whether MCP is on, `{"on":<bool>}`.
///
/// # Errors
///
/// What [`read`] refuses, journaled as the store's refusal and answered
/// with its code.
pub(crate) async fn mcp_read<E: Environment>(shared: &Shared<E>) -> Result<String, Failure> {
    read(&*shared.store, &shared.realm)
        .await
        .map(|settings| written(settings.mcp))
        .map_err(|refusal| Failure::new(refusal, "store-refused"))
}

/// `settings.mcp.set`: MCP switched as `args` says, staged in the tree's unit
/// over what the store holds committed now, and answered as it will stand.
///
/// # Errors
///
/// `invalid` when `args` holds no boolean `on`, which the schema refuses
/// first; what the store refused the read with; `internal` with
/// `[NC-KERN-0008]` when the tree holds no unit, which the decision point
/// opened for the command; and the unit's own bounds.
pub(crate) async fn mcp_set<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    args: &serde_json::Value,
) -> Result<String, Failure> {
    let on = asked(args)?;
    let committed = shared
        .store
        .get(&shared.realm, HOLDER, MCP)
        .await
        .map_err(|failed| Failure::new(units::store_failed(&failed), "store-refused"))?;
    let mut tree = dispatch::lock(tree);
    let unit = tree.unit_mut().ok_or_else(|| {
        Failure::new(
            Refusal::new(
                RefusalKind::Internal,
                ErrorCode::KernelUnitAborted,
                format!("`{MCP_SET}` ran outside the unit the decision point opened for it"),
            ),
            "unit-lost",
        )
    })?;
    let id = unit.unit();
    unit.overlay_mut()
        .stage(id, HOLDER, MCP, committed.as_ref(), Some(written(on)))
        .map_err(|refusal| Failure::new(refusal, "bound-reached"))?;
    Ok(written(on))
}

/// The `on` a set asks for.
///
/// # Errors
///
/// `invalid` when `args` holds no boolean `on`: a set never guesses which
/// way to switch.
fn asked(args: &serde_json::Value) -> Result<bool, Failure> {
    args.get(ON)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            Failure::new(
                Refusal::new(
                    RefusalKind::Invalid,
                    ErrorCode::ValidationArgumentsInvalid,
                    format!(
                        "`{MCP_SET}` was asked with no boolean `{ON}`, so nothing was switched"
                    ),
                ),
                "arguments-invalid",
            )
        })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use nc_local::{Expect, Quotas, StagedWrite, UnitKind, UnitMeta, UnitRecord};

    use super::{
        HOLDER, MCP, MCP_READ, MCP_SET, ON, Settings, asked, read, writes_only_its_entry, written,
    };
    use crate::backend::{MemoryBackend, StoreBackend as _};
    use crate::builtin::Builtin;
    use crate::dispatch::RefusalKind;

    /// Every argument `settings.mcp.read` reads, by the name the command
    /// declares: none.
    const READ_READS: [&str; 0] = [];

    /// Every argument `settings.mcp.set` reads, by the name the command
    /// declares.
    const SET_READS: [&str; 1] = [ON];

    /// The declared settings are the ones this module implements.
    ///
    /// **Both ways**, against `Command::arguments`, which the compiler emits
    /// from the parse it validates the schema with, and the decision each is
    /// made under: `Read` and `Write` on the platform's `mcp` feature, never
    /// the realm, the set a change of state that can be undone and the read
    /// none — so a unit is opened for the set alone, and the permit that
    /// decides them is a person's.
    #[test]
    fn the_declared_settings_are_the_ones_this_module_implements() {
        for (name, reads, action, mutating) in [
            (MCP_READ, &READ_READS[..], "Read", false),
            (MCP_SET, &SET_READS[..], "Write", true),
        ] {
            let declared =
                nc_catalog::command(name).unwrap_or_else(|| panic!("`{name}` is declared"));
            let takes: BTreeSet<&str> = declared.arguments.iter().copied().collect();
            let read: BTreeSet<&str> = reads.iter().copied().collect();
            assert_eq!(takes, read, "`{name}` declares {:?}", declared.arguments);
            assert_eq!(declared.arguments.len(), reads.len(), "`{name}`");
            assert_eq!(
                (
                    declared.action,
                    declared.resource_selector,
                    declared.mutating
                ),
                (action, "feature:mcp", mutating),
                "`{name}`"
            );
            assert_eq!(
                declared.categories,
                ["noctua.settings"],
                "`{name}` is not listed among the settings"
            );
        }
        assert!(matches!(
            nc_catalog::command(MCP_READ)
                .expect("declared")
                .reversibility,
            nc_catalog::Reversibility::Read
        ));
        assert!(matches!(
            nc_catalog::command(MCP_SET)
                .expect("declared")
                .reversibility,
            nc_catalog::Reversibility::Write
        ));
        assert_eq!(
            (MCP_READ, MCP_SET, ON, MCP),
            (
                "settings.mcp.read",
                "settings.mcp.set",
                "on",
                "settings.mcp"
            )
        );
        assert_eq!(Builtin::named(MCP_READ), Some(Builtin::McpRead));
        assert_eq!(Builtin::named(MCP_SET), Some(Builtin::McpSet));
    }

    /// The settings are held under no identity a plugin can have.
    ///
    /// A plugin's identity is `<word>:<word>`, and the store keys every entry
    /// by the identity it is kept for, so no plugin reads, writes or lists the
    /// kernel's.
    #[test]
    fn the_settings_are_held_under_no_identity_a_plugin_can_have() {
        assert!(!crate::manifest::id_is_well_formed(HOLDER), "`{HOLDER}`");
        assert!(
            crate::manifest::id_is_well_formed("noctua:kernel"),
            "the rule refuses every identity, and so tells nothing of this one"
        );
    }

    /// The world the policy cases are about, which the store keys `home` by.
    fn world() -> nc_authz::MemoryDirectory {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        nc_authz::MemoryDirectory::from_world(include_str!("../../../policy/world.json"), &pdp)
            .expect("the world loads")
    }

    /// A store holding `value` as MCP's entry, committed by a unit nobody
    /// dispatched, and the realm it holds it in.
    async fn holding(value: &str) -> (MemoryBackend, nc_ids::OpaqueId) {
        let world = world();
        let realm = world.realm_id("home").expect("home is a realm");
        let store = MemoryBackend::default();
        let unit = UnitRecord {
            unit: nc_ids::Minter::new(
                nc_ids::MachineFlake::from_value(1).expect("a flake"),
                nc_ids::SystemClock,
            )
            .mint()
            .expect("a unit"),
            meta: UnitMeta {
                realm: realm.clone(),
                principal: world
                    .principal(nc_authz::PrincipalKind::User, "ana")
                    .expect("ana is a user")
                    .id()
                    .clone(),
                kind: UnitKind::Step,
                label: "a test's".to_owned(),
                committed_at_ms: 0,
            },
            writes: vec![StagedWrite {
                plugin: HOLDER.to_owned(),
                key: MCP.to_owned(),
                expect: Expect::Anything,
                after: Some(value.to_owned()),
            }],
            blobs: Vec::new(),
            unpinned: Vec::new(),
            notes: Vec::new(),
            marks: Vec::new(),
        };
        let quotas = Quotas {
            entries_per_plugin: 10,
            lowered: BTreeMap::new(),
            blobs: 10,
            writes_per_unit: 10,
            notes_per_unit: 10,
        };
        store
            .commit(&unit, &quotas)
            .await
            .expect("the entry commits");
        (store, realm)
    }

    /// A setting never written is its default, and MCP is off.
    ///
    /// **Off until a person turns it on** (`NOCTUA.md §20.3`), and what the two
    /// texts this module writes are read back as, so a store holding nothing
    /// is the one case that answers the default.
    #[tokio::test]
    async fn a_setting_never_written_is_its_default_and_mcp_is_off() {
        let realm = world().realm_id("home").expect("home is a realm");
        let empty = MemoryBackend::default();
        assert_eq!(
            read(&empty, &realm).await.expect("an empty store is read"),
            Settings { mcp: false }
        );
        for on in [true, false] {
            let (store, realm) = holding(&written(on)).await;
            assert_eq!(
                read(&store, &realm).await.expect("a written entry is read"),
                Settings { mcp: on },
                "{}",
                written(on)
            );
        }
        assert_eq!(written(true), r#"{"on":true}"#);
    }

    /// A setting no writer here wrote is refused, and never read as on or off.
    ///
    /// **Each shape an entry the store keeps could hold**: a bare boolean or
    /// text, `on` of another type or missing, and a field beside it — each
    /// refused as a row the kernel did not write, its text repeated cut and
    /// escaped. The store keeps canonical JSON alone, so no spelling of the two
    /// texts written here is another.
    #[tokio::test]
    async fn a_setting_no_writer_here_wrote_is_refused_and_never_read_as_on_or_off() {
        for value in [
            "true",
            r#"{"on":1}"#,
            r#"{"on":true,"x":1}"#,
            r#"{"on":false,"x":1}"#,
            r#"{"on":null}"#,
            r#""on""#,
            "{}",
        ] {
            let (store, realm) = holding(value).await;
            let refused = read(&store, &realm)
                .await
                .expect_err("an entry nothing here writes");
            assert_eq!(
                (refused.kind(), refused.code()),
                (
                    RefusalKind::Internal,
                    nc_local::LocalError::Unexpected(String::new()).code()
                ),
                "{value}"
            );
            assert!(
                refused
                    .sentence()
                    .contains(&crate::registry::quoted_up_to(value, 64)),
                "{value}: {}",
                refused.sentence()
            );
        }
    }

    /// A set never guesses which way to switch.
    ///
    /// The schema refuses a set without a boolean `on` before the handler
    /// runs; the handler refuses it again rather than read a default.
    #[test]
    fn a_set_with_no_boolean_on_is_refused_rather_than_read_as_off() {
        assert_eq!(asked(&serde_json::json!({"on": true})).ok(), Some(true));
        assert_eq!(asked(&serde_json::json!({"on": false})).ok(), Some(false));
        for args in [
            serde_json::json!({}),
            serde_json::json!({"on": "false"}),
            serde_json::json!({"on": 0}),
        ] {
            let refused = asked(&args).expect_err("no boolean `on`");
            assert_eq!(
                (refused.refusal().kind(), refused.word()),
                (RefusalKind::Invalid, "arguments-invalid"),
                "{args}"
            );
        }
    }

    /// Only the set of MCP writes nothing but its entry.
    ///
    /// Named by the command, never by the namespace: the read writes nothing
    /// at all, and a command of another namespace is not a setting's.
    #[test]
    fn only_mcps_set_writes_only_its_entry() {
        assert!(writes_only_its_entry(MCP_SET));
        for name in [MCP_READ, "settings.other.set", "limits.prune"] {
            assert!(!writes_only_its_entry(name), "`{name}`");
        }
    }
}

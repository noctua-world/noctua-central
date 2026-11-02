//! A capability that was not granted answers `denied`, for every function of
//! the world, and one that was granted never does — the two halves of
//! `NOCTUA.md §11.3`'s deny-by-default, acceptance criterion 4.
//!
//! **No list here is written by hand.** The world is read with its own parser
//! (`wit-parser`) from `crates/nc-kernel/wit`, the catalog's division of it is
//! `nc_catalog::CAPABILITIES` and `UNGATED`, the reference plugin's probe
//! names what it calls in `PROBED`, and the kernel records what each host
//! function answered to as it runs. Each is held to the others: a function the
//! world gains that the catalog does not write down — under a capability, or
//! among the functions an interface given to every plugin lists beside its
//! reason — a host function that answers without asking for its capability, a
//! capability whose functions the world does not have, and a probe that stops
//! calling something are each a failure here.
//!
//! **Through the real plugin and its real manifest.** The probe runs inside
//! the component `cargo xtask build --plugins` wrote, granted what
//! `plugins/reference/plugin.toml` asks for — nothing — so every refusal below
//! crossed the boundary as the world's `error` value and came back. **Granted
//! everything, it runs as a command of a whole kernel** — through the decision
//! point, in a call tree, for a person, in a unit of work — where every
//! function either answers or refuses what it was asked, and none is refused
//! for its capability or for where it was called from.
//!
//! A resource's method is reachable only through the function that hands the
//! resource out (`open`, `create`, `begin`); with that function refused there
//! is no handle to call it on, which the probe reports as `unreachable` and
//! this suite holds to the constructor's own denial.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nc_catalog::{CAPABILITIES, Capability, ErrorCode, UNGATED};
use nc_kernel::capability::{GrantSet, Linked};
use nc_kernel::engine::{Compiled, Engine};
use nc_kernel::logging::Captured;
use nc_kernel::manifest::PluginManifest;
use nc_plugin_reference::behaviour::PROBED;

/// The world `plugin`'s imports, as its own parser reads them.
struct World {
    /// Every function, as `interface#function`, with the resource it is a
    /// method of when it is one.
    functions: BTreeMap<String, Option<String>>,
    /// Every function that hands out a resource, as `interface#function`, by
    /// the `interface#resource` it hands out.
    constructors: BTreeMap<String, Vec<String>>,
    /// Every interface the world imports.
    interfaces: BTreeSet<String>,
}

/// Reads `crates/nc-kernel/wit` and walks the imports of the world `plugin`.
fn world() -> World {
    use wit_parser::{FunctionKind, Handle, Resolve, Type, TypeDefKind, WorldItem};

    let mut resolve = Resolve::default();
    let (package, _) = resolve
        .push_path(concat!(env!("CARGO_MANIFEST_DIR"), "/wit"))
        .expect("the world parses");
    let plugin = resolve
        .select_world(&[package], Some("plugin"))
        .expect("the world `plugin` is there");
    let mut world = World {
        functions: BTreeMap::new(),
        constructors: BTreeMap::new(),
        interfaces: BTreeSet::new(),
    };
    for item in resolve.worlds[plugin].imports.values() {
        let WorldItem::Interface { id, .. } = item else {
            panic!("the world imports something that is not an interface: {item:?}");
        };
        let interface = &resolve.interfaces[*id];
        let name = interface
            .name
            .clone()
            .expect("every interface the world imports is named");
        world.interfaces.insert(name.clone());
        for function in interface.functions.values() {
            let resource = match function.kind {
                FunctionKind::Method(resource) => resolve.types[resource].name.clone(),
                _ => None,
            };
            // A function that hands out a resource answers `result<own<R>, _>`.
            let handed_out = function.result.and_then(|result| match result {
                Type::Id(id) => match &resolve.types[id].kind {
                    TypeDefKind::Result(result) => match result.ok {
                        Some(Type::Id(handle)) => match resolve.types[handle].kind {
                            TypeDefKind::Handle(Handle::Own(owned)) => {
                                resolve.types[owned].name.clone()
                            }
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            });
            if let Some(handed_out) = handed_out {
                world
                    .constructors
                    .entry(format!("{name}#{handed_out}"))
                    .or_default()
                    .push(format!("{name}#{}", function.name));
            }
            world
                .functions
                .insert(format!("{name}#{}", function.name), resource);
        }
    }
    world
}

/// The capability the catalog says `function` of `interface` answers to, or
/// none for a function the catalog gives every plugin.
///
/// # Panics
///
/// When the catalog places it nowhere, or in more than one place. **An
/// interface given to every plugin gives only the functions it lists**, so a
/// function the world gains there is placed nowhere until someone writes it
/// beside a reason that covers it — taking the interface as the gift made the
/// reason cover functions written after it.
fn answers_to(interface: &str, function: &str) -> Option<Capability> {
    let granting: Vec<Capability> = CAPABILITIES
        .iter()
        .filter(|spec| spec.interface == interface && spec.functions.contains(&function))
        .map(|spec| spec.capability)
        .collect();
    let given = UNGATED
        .iter()
        .any(|ungated| ungated.interface == interface && ungated.functions.contains(&function));
    match (granting.as_slice(), given) {
        ([one], false) => Some(*one),
        ([], true) => None,
        _ => panic!(
            "`{interface}#{function}` is granted by {granting:?} and given to every plugin: \
             {given} — the catalog places one function in exactly one place"
        ),
    }
}

/// The reference plugin's component, compiled by this kernel's engine.
fn the_reference_plugin() -> (Engine, Compiled) {
    let engine = Engine::new().expect("the engine's configuration is valid");
    let compiled = engine
        .compile(&nc_kernel::component_for_tests("reference"))
        .expect("the component the build wrote compiles");
    (engine, compiled)
}

/// What the probe answered for every function, holding `grants`, and what the
/// kernel recorded while it ran.
async fn probed(grants: &GrantSet) -> (BTreeMap<String, String>, nc_kernel::ExecutedForTests) {
    let (engine, compiled) = the_reference_plugin();
    let executed = nc_kernel::execute_for_tests(
        &engine,
        &compiled,
        grants,
        "reference.probe",
        r#"{"note":"photo"}"#,
    )
    .await
    .expect("the probe runs to the end without trapping");
    let answered = executed
        .answer
        .clone()
        .unwrap_or_else(|refusal| panic!("the probe was refused: {refusal}"));
    let read = nc_canon::parse(&answered)
        .expect("the probe answers canonical JSON")
        .into_object()
        .expect("the probe answers an object");
    let outcomes = read
        .into_iter()
        .map(|(function, outcome)| {
            let outcome = outcome
                .into_text()
                .unwrap_or_else(|| panic!("`{function}`'s outcome is not text"));
            (function, outcome)
        })
        .collect();
    (outcomes, executed)
}

/// The sentence `[NC-PLUG-0001]` refuses `interface#function` with, from its
/// opening to the capability it names.
fn denial_of(interface: &str, function: &str, capability: Capability) -> String {
    format!(
        "denied: [{}] `{interface}#{function}` answers to the capability `{}`",
        ErrorCode::PluginCapabilityDenied,
        capability.spec().name
    )
}

#[tokio::test]
async fn every_function_of_every_capability_in_the_world_returns_denied_when_not_granted() {
    let manifest = PluginManifest::parse(include_str!("../../../plugins/reference/plugin.toml"))
        .expect("the reference plugin's manifest is one the kernel reads");
    assert_eq!(
        manifest.grants(),
        &GrantSet::none(),
        "this proves the denied half only for a plugin granted nothing, and \
         plugins/reference/plugin.toml now asks for something"
    );
    let world = world();
    let (outcomes, _) = probed(manifest.grants()).await;
    let (mut denied, mut unreachable, mut answered) = (0_usize, 0_usize, 0_usize);
    for (function, resource) in &world.functions {
        let outcome = outcomes
            .get(function)
            .unwrap_or_else(|| panic!("the probe did not answer for `{function}`"));
        let (interface, name) = function.split_once('#').expect("interface#function");
        match (answers_to(interface, name), resource) {
            (None, _) => {
                assert_eq!(outcome, "answered", "`{function}` is given to every plugin");
                answered += 1;
            }
            (Some(capability), Some(resource)) => {
                assert_eq!(
                    outcome, "unreachable",
                    "`{function}` is a method of `{resource}`, and a plugin granted nothing \
                     holds no `{resource}` to call it on"
                );
                // **And the reason it is unreachable is a denial**, of every
                // function that hands the resource out, under the same
                // capability — so a method behind a constructor that answered
                // anything else is not proved denied by being unreached.
                let constructors = world
                    .constructors
                    .get(&format!("{interface}#{resource}"))
                    .unwrap_or_else(|| panic!("nothing hands out `{resource}`"));
                for constructor in constructors {
                    let (_, built) = constructor.split_once('#').expect("interface#function");
                    assert_eq!(
                        answers_to(interface, built),
                        Some(capability),
                        "`{constructor}` hands out what `{function}` is called on, under another \
                         capability"
                    );
                    assert!(
                        outcomes[constructor].starts_with(&denial_of(interface, built, capability)),
                        "`{constructor}` answered {:?}",
                        outcomes[constructor]
                    );
                }
                unreachable += 1;
            }
            (Some(capability), None) => {
                assert!(
                    outcome.starts_with(&denial_of(interface, name, capability)),
                    "`{function}` answered {outcome:?}, and its capability `{}` was not granted",
                    capability.spec().name
                );
                denied += 1;
            }
        }
    }
    // **Each count against what the world says it should be**, so a walk that
    // judged fewer than every function of a kind is a count that disagrees,
    // not a sum that still adds up.
    let methods = world
        .functions
        .values()
        .filter(|resource| resource.is_some())
        .count();
    let given = world
        .functions
        .keys()
        .filter(|function| {
            UNGATED.iter().any(|ungated| {
                ungated
                    .functions
                    .iter()
                    .any(|given| function.split_once('#') == Some((ungated.interface, *given)))
            })
        })
        .count();
    assert_eq!(
        (denied, unreachable, answered),
        (world.functions.len() - methods - given, methods, given),
        "every function of the world was judged once, as its kind says"
    );
    assert!(
        denied > world.interfaces.len() && methods > 0 && given > 0,
        "{denied} denied, {methods} method(s) and {given} given over {} function(s): a \
         world this suite read part of",
        world.functions.len()
    );
}

/// The functions of the world whose answer to the granted probe is a refusal
/// of what it was asked, each with its case and its code — every other
/// function answers, or is a method of a resource one of these did not hand
/// out.
///
/// `open` of a blob no machine this kernel reaches holds, and `announce`,
/// which no peer hears; a compensation naming a command that only reads; and
/// a surface and a timer the probe names and does not hold.
const REFUSED_AS_ASKED: [(&str, &str, ErrorCode); 6] = [
    ("blobs#open", "unavailable", ErrorCode::BlobNotLocal),
    ("blobs#announce", "unavailable", ErrorCode::BlobNoPeers),
    (
        "commands#[method]unit.compensate-with",
        "invalid",
        ErrorCode::KernelCompensationRefused,
    ),
    ("ui#close-surface", "not-found", ErrorCode::PluginNotFound),
    ("ui#render", "not-found", ErrorCode::PluginNotFound),
    ("timers#cancel", "not-found", ErrorCode::PluginNotFound),
];

/// A clock standing at `2026-08-26T00:00:00Z`, inside the grant `g.current`,
/// which moves only past what an identifier asks it to.
#[derive(Debug, Clone)]
struct Standing(Arc<AtomicU64>);

impl nc_ids::Clock for Standing {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    fn wait_past(&self, millis: u64) {
        self.0.fetch_max(millis.saturating_add(1), Ordering::SeqCst);
    }
}

/// A kernel whose plugins' data is held in memory, deciding with the family
/// pack over the world's directory.
#[derive(Debug)]
struct InMemory;

impl nc_kernel::Environment for InMemory {
    type Store = nc_kernel::backend::MemoryBackend;
    type Blobs = nc_kernel::blobs::MemoryBlobs;
    type Journal = nc_journal::InMemory;
    type Clock = Standing;
    type Directory = nc_authz::MemoryDirectory;
    type Log = Arc<Captured>;
    type Transport = nc_kernel::net::Scripted;
}

/// A kernel over [`InMemory`], with the reference plugin loaded granted every
/// capability, `ana` of the world, and the log the kernel writes to.
async fn granted_everything() -> (
    nc_kernel::Kernel<InMemory>,
    nc_authz::Principal,
    Arc<Captured>,
) {
    let pack = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
    let world_json = include_str!("../../../policy/world.json");
    let directory = nc_authz::MemoryDirectory::from_world(world_json, &pack)
        .expect("the world is valid")
        .with_epoch("home", 7)
        .with_grant(
            "home",
            nc_authz::EntityRef {
                type_name: "User".to_owned(),
                id: "ana".to_owned(),
            },
            "g.current",
        )
        .expect("the realm holds the grant");
    let people =
        nc_authz::MemoryDirectory::from_world(world_json, &pack).expect("the world is valid");
    let ana = people
        .principal(nc_authz::PrincipalKind::User, "ana")
        .expect("ana is a user");
    let clock = Standing(Arc::new(AtomicU64::new(1_787_702_400_000)));
    let log = Arc::new(Captured::default());
    let kernel = nc_kernel::Kernel::<InMemory>::open(nc_kernel::KernelConfig {
        store: nc_kernel::backend::MemoryBackend::default(),
        blobs: nc_kernel::blobs::MemoryBlobs::default(),
        pdp: Ok(pack),
        journal: nc_journal::Journal::new(
            nc_journal::InMemory::default(),
            people.realm_id("home").expect("home is a realm"),
            ana.id().clone(),
            nc_ids::Minter::new(
                nc_ids::MachineFlake::from_value(3).expect("a flake"),
                clock.clone(),
            ),
        ),
        directory,
        clock,
        log: Arc::clone(&log),
        transport: nc_kernel::net::Scripted::default(),
    })
    .await
    .expect("the kernel opens");
    let every: Vec<String> = CAPABILITIES
        .iter()
        .map(|spec| format!("\"{}\"", spec.name))
        .collect();
    let manifest = PluginManifest::parse(&format!(
        "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = [{}]\n\n[net]\n\
         domains = [\"example.com\"]\n\n[events]\nsubscribe = [\"reference.probed\"]\n",
        every.join(", ")
    ))
    .expect("a manifest granting everything");
    assert_eq!(manifest.grants(), &GrantSet::every());
    kernel
        .load(manifest, &nc_kernel::component_for_tests("reference"))
        .await
        .expect("the reference plugin loads granted everything");
    (kernel, ana, log)
}

#[tokio::test]
async fn the_same_probe_meets_no_denial_when_everything_is_granted() {
    // **The discriminating half.** A host function that refused whatever was
    // granted would pass the test above; granted everything and run as a
    // command through the decision point — in a call tree, for a person, in a
    // unit of work — the same probe must meet no `denied` at all, and every
    // function must answer what it was asked, or refuse what it was asked as
    // the one it is: none says it is not built, and none is refused for where
    // it was called from.
    let (kernel, ana, log) = granted_everything().await;
    let probed = kernel
        .dispatch(
            &nc_kernel::Caller::new(ana),
            "reference.probe",
            r#"{"note":"photo"}"#,
        )
        .await
        .expect("the probe answers");
    let outcomes: BTreeMap<String, String> = nc_canon::parse(&probed)
        .expect("canonical")
        .into_object()
        .expect("an object")
        .into_iter()
        .map(|(function, outcome)| (function, outcome.into_text().expect("text")))
        .collect();
    let world = world();
    let (mut answered, mut refused, mut unreached) = (0_usize, 0_usize, 0_usize);
    for (function, resource) in &world.functions {
        let outcome = &outcomes[function];
        assert!(
            !outcome.starts_with("denied"),
            "`{function}` answered {outcome:?} with every capability granted"
        );
        if let Some((_, case, code)) = REFUSED_AS_ASKED
            .iter()
            .find(|(named, ..)| named == function)
        {
            assert!(
                outcome.starts_with(&format!("{case}: [{code}] ")),
                "`{function}` answered {outcome:?}, and is built to refuse what the probe asks \
                 it as {case} with {code}"
            );
            refused += 1;
        } else if outcome == "unreachable" {
            // **A method no handle was handed out for**, behind a constructor
            // refused for what it was asked — never for a capability.
            let (interface, _) = function.split_once('#').expect("interface#function");
            let resource = resource.as_ref().unwrap_or_else(|| {
                panic!("`{function}` was not reached, and is no method of a resource")
            });
            for constructor in &world.constructors[&format!("{interface}#{resource}")] {
                assert!(
                    REFUSED_AS_ASKED
                        .iter()
                        .any(|(named, ..)| named == constructor),
                    "`{constructor}` handed out nothing `{function}` could be called on, \
                     answering {:?}",
                    outcomes[constructor]
                );
            }
            unreached += 1;
        } else {
            assert_eq!(outcome, "answered", "`{function}` is built and granted");
            answered += 1;
        }
    }
    assert_eq!(answered + refused + unreached, world.functions.len());
    assert_eq!(
        refused,
        REFUSED_AS_ASKED.len(),
        "each refusal named was met"
    );
    assert!(
        answered > world.interfaces.len() * 2 && unreached > 0,
        "{answered} answered and {unreached} unreached of {} function(s)",
        world.functions.len()
    );
    let seen = log.named("kernel.capability_denied");
    assert!(
        seen.is_empty(),
        "a denial was logged with everything granted: {seen:?}"
    );
}

#[test]
fn the_capability_registry_the_linker_and_the_world_name_the_same_functions() {
    let world = world();
    let in_the_world: BTreeSet<&str> = world.functions.keys().map(String::as_str).collect();

    // The catalog's division of the world, both ways: every function of every
    // capability is the world's, every interface given to every plugin is
    // one the world imports and every function it gives is the world's, and
    // every function of the world is in exactly one place (`answers_to` panics
    // on none and on two).
    let mut in_the_catalog: BTreeSet<String> = BTreeSet::new();
    for spec in CAPABILITIES {
        assert!(
            world.interfaces.contains(spec.interface),
            "`{}` grants the interface `{}`, which the world does not import",
            spec.name,
            spec.interface
        );
        for function in spec.functions {
            assert!(
                in_the_catalog.insert(format!("{}#{function}", spec.interface)),
                "`{}#{function}` is granted twice",
                spec.interface
            );
        }
    }
    for ungated in UNGATED {
        assert!(
            world.interfaces.contains(ungated.interface),
            "`{}` is given to every plugin, and the world does not import it",
            ungated.interface
        );
        // **Read from what the catalog lists, never from the world.** This
        // half was the world's own functions of the interface, so it equalled
        // the world by construction: a `log#flush` the world gained, with a
        // host function and a row of the probe, passed every test here and
        // answered a plugin granted nothing, under a reason written before it.
        for function in ungated.functions {
            assert!(
                in_the_catalog.insert(format!("{}#{function}", ungated.interface)),
                "`{}#{function}` is placed twice",
                ungated.interface
            );
        }
    }
    let gated = in_the_world
        .iter()
        .filter(|function| {
            let (interface, name) = function.split_once('#').expect("interface#function");
            answers_to(interface, name).is_some()
        })
        .count();
    assert_eq!(
        gated,
        CAPABILITIES
            .iter()
            .map(|spec| spec.functions.len())
            .sum::<usize>(),
        "the world's functions a capability grants are not the ones the catalog grants"
    );
    assert_eq!(
        in_the_catalog
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        in_the_world,
        "catalog/capabilities.toml and the world divide different sets of functions"
    );
    // Every interface the world imports is placed: an interface of only types
    // has no function to be granted, so it has to be given.
    for interface in &world.interfaces {
        assert!(
            CAPABILITIES.iter().any(|spec| spec.interface == interface)
                || UNGATED.iter().any(|ungated| ungated.interface == interface),
            "the world imports `{interface}`, and the catalog neither gates nor gives it"
        );
    }

    // The probe calls every one of them.
    assert_eq!(
        PROBED.iter().copied().collect::<BTreeSet<_>>(),
        in_the_world,
        "the reference plugin's probe and the world name different functions"
    );

    // And the linker: the component imports exactly the world's functions —
    // which it can only do because the probe calls each — and links against
    // this kernel's host, or `instantiate_pre` would have refused it.
    let (engine, compiled) = the_reference_plugin();
    let version = nc_kernel::world::WORLD_VERSION;
    let imported: BTreeSet<String> = compiled.imports(&engine).into_iter().collect();
    let expected: BTreeSet<String> = in_the_world
        .iter()
        .map(|function| {
            let (interface, name) = function.split_once('#').expect("interface#function");
            format!("noctua:kernel/{interface}@{version}#{name}")
        })
        .collect();
    assert_eq!(
        imported, expected,
        "the reference component imports other functions than the world declares"
    );
    assert!(
        in_the_world.len() > world.interfaces.len(),
        "{} function(s) over {} interface(s) were read",
        in_the_world.len(),
        world.interfaces.len()
    );
}

#[tokio::test]
async fn every_host_function_reached_asked_for_the_capability_the_catalog_names() {
    // **`LINKED`, which is what finds a host function that forgot to ask.** The
    // kernel records each function as it asks for its capability, so a
    // function that answered without asking is one the record does not hold;
    // and it records which capability it asked for, so one that asked for the
    // wrong one is a record the catalog disagrees with.
    let world = world();
    for grants in [GrantSet::none(), GrantSet::every()] {
        let (outcomes, executed) = probed(&grants).await;
        let reached: BTreeSet<Linked> = executed.reached.iter().copied().collect();
        let expected: BTreeSet<(String, String, Option<Capability>)> = world
            .functions
            .iter()
            .filter(|(function, _)| outcomes[*function] != "unreachable")
            .map(|(function, _)| {
                let (interface, name) = function.split_once('#').expect("interface#function");
                (
                    interface.to_owned(),
                    name.to_owned(),
                    answers_to(interface, name),
                )
            })
            .collect();
        let recorded: BTreeSet<(String, String, Option<Capability>)> = reached
            .iter()
            .map(|linked| {
                (
                    linked.interface.to_owned(),
                    linked.function.to_owned(),
                    linked.capability,
                )
            })
            .collect();
        assert_eq!(
            recorded, expected,
            "the host functions the probe reached are not the ones it called, each under the \
             capability the catalog names"
        );
        assert!(
            recorded.len() > world.interfaces.len(),
            "{} function(s) were recorded over {} interface(s)",
            recorded.len(),
            world.interfaces.len()
        );
    }
}

#[tokio::test]
async fn a_denial_is_recorded_with_its_code_and_its_capability() {
    // What the kernel will log as `kernel.capability_denied`: one entry per
    // refused function, carrying its code, the capability it was refused and
    // how many times it was.
    let world = world();
    let (outcomes, executed) = probed(&GrantSet::none()).await;
    // **Each function's count, from what the probe answered.** A function that
    // answered `denied` was refused once. A method the probe could not reach
    // was attempted by asking for its resource first, which is one more refused
    // call of the function that hands the resource out.
    let mut expected: BTreeMap<String, u64> = BTreeMap::new();
    let (mut denied, mut unreachable) = (0_u64, 0_u64);
    for (function, outcome) in &outcomes {
        if outcome.starts_with("denied") {
            *expected.entry(function.clone()).or_default() += 1;
            denied += 1;
        } else if outcome == "unreachable" {
            let (interface, _) = function.split_once('#').expect("interface#function");
            let resource = world.functions[function]
                .as_ref()
                .unwrap_or_else(|| panic!("`{function}` was not reached and is not a method"));
            let [constructor] = world.constructors[&format!("{interface}#{resource}")].as_slice()
            else {
                panic!("more than one function hands out `{resource}`: which did the probe ask?");
            };
            *expected.entry(constructor.clone()).or_default() += 1;
            unreachable += 1;
        }
    }
    let mut recorded: BTreeMap<String, u64> = BTreeMap::new();
    for (denial, times) in &executed.denied {
        assert_eq!(denial.code(), ErrorCode::PluginCapabilityDenied);
        let interface = denial.capability().spec().interface;
        assert_eq!(
            answers_to(interface, denial.function()),
            Some(denial.capability()),
            "`{interface}#{}` was refused under a capability the catalog does not give it",
            denial.function()
        );
        assert!(
            recorded
                .insert(format!("{interface}#{}", denial.function()), *times)
                .is_none(),
            "`{interface}#{}` is recorded twice, so the record grows with the calls refused",
            denial.function()
        );
    }
    assert_eq!(
        recorded, expected,
        "each refused function is recorded once, with how many times it was refused"
    );
    assert_eq!(
        recorded.values().sum::<u64>(),
        denied + unreachable,
        "every refused call is counted"
    );
    assert!(
        !recorded.is_empty()
            && recorded.len() < world.functions.len()
            && recorded.values().any(|times| *times > 1),
        "{} function(s) refused of {}, none of them twice: a probe that no longer \
         judges the count",
        recorded.len(),
        world.functions.len()
    );
    // And the one interface every plugin is given reached the plugin's own
    // ring, not the central log.
    assert_eq!(
        executed.log,
        [("info".to_owned(), "the probe reached the log".to_owned())]
    );
}

//! A plugin is loaded only when every command it declares at `activate` can be
//! authorized and is its own to declare — and a descriptor missing a field is
//! rejected there, naming the command (`NOCTUA.md §11.7`, acceptance
//! criterion 5).
//!
//! **Through the real component, one build for every case.** The reference
//! plugin reads how to activate from its own store's `activation` key, which
//! each test commits through the store before the plugin is loaded: the
//! namespace to declare under, and the one way to break its first command. So
//! every refusal below crossed the boundary as a declaration the component
//! made, was judged by the kernel, and left nothing registered.

use nc_authz::{MemoryDirectory, Pdp, PrincipalKind};
use nc_catalog::ErrorCode;
use nc_ids::{MachineFlake, Minter, OpaqueId, Snowflake, SystemClock};
use nc_kernel::backend::{MemoryBackend, StoreBackend};
use nc_kernel::manifest::PluginManifest;
use nc_kernel::registry::Owner;
use nc_kernel::{Environment, Kernel, KernelConfig, LoadError};
use nc_local::{
    BlobRow, CommitError, Committed, Entry, Expect, LocalError, Page, Quotas, StagedWrite,
    UnitDetail, UnitKind, UnitMeta, UnitRecord, UnitRow, UnitState,
};
use nc_plugin_reference::behaviour::{
    ACTIVATION, Activation, Reversibility, Variant, commands_for,
};

/// The world every policy case is about, which mints the identifiers a seed is
/// committed under.
const WORLD: &str = include_str!("../../../policy/world.json");

/// A kernel whose plugins' data is held in memory.
#[derive(Debug)]
struct InMemory;

impl Environment for InMemory {
    type Store = MemoryBackend;
    type Blobs = nc_kernel::blobs::MemoryBlobs;
    type Journal = nc_journal::InMemory;
    type Clock = SystemClock;
    type Directory = MemoryDirectory;
    type Log = nc_kernel::logging::Captured;
    type Transport = nc_kernel::net::Scripted;
}

/// A kernel whose plugins' data is in the local database, held in memory.
#[derive(Debug)]
struct InTheLocalDatabase;

impl Environment for InTheLocalDatabase {
    type Store = nc_local::LocalStore;
    type Blobs = nc_kernel::blobs::MemoryBlobs;
    type Journal = nc_journal::InMemory;
    type Clock = SystemClock;
    type Directory = MemoryDirectory;
    type Log = nc_kernel::logging::Captured;
    type Transport = nc_kernel::net::Scripted;
}

/// The names `activation` declares, in the order the registry lists them.
fn names_for(activation: &Activation) -> Vec<String> {
    let mut names: Vec<String> = commands_for(activation)
        .into_iter()
        .map(|command| command.name)
        .collect();
    names.sort();
    names
}

/// The names the reference plugin declares under `namespace`, as written.
fn written_under(namespace: &str) -> Vec<String> {
    names_for(&Activation {
        namespace: namespace.to_owned(),
        variant: Variant::Normal,
        subscribe: Vec::new(),
    })
}

/// The reference plugin's manifest, granted its store so `activate` can read
/// how to activate, under `namespace` and the identity `id`.
fn manifest(id: &str, namespace: &str) -> PluginManifest {
    PluginManifest::parse(&format!(
        "id = \"{id}\"\nnamespace = \"{namespace}\"\ncapabilities = [\"store\"]\n"
    ))
    .expect("a manifest the kernel reads")
}

/// What a seed is committed as: one unit of the realm `home`, by `ana`, with
/// identifiers the directory minted — the kernel never mints one from text.
struct Seeder {
    minter: Minter<SystemClock>,
    realm: OpaqueId,
    principal: OpaqueId,
}

impl Seeder {
    fn new() -> Self {
        let pdp = Pdp::embedded("family").expect("the family pack is carried");
        let directory = MemoryDirectory::from_world(WORLD, &pdp).expect("the world is valid");
        Self {
            minter: Minter::new(MachineFlake::from_value(1).expect("a flake"), SystemClock),
            realm: directory.realm_id("home").expect("home is a realm"),
            principal: directory
                .principal(PrincipalKind::User, "ana")
                .expect("ana is a user")
                .id()
                .clone(),
        }
    }

    /// Commits `value` as `plugin`'s `activation` in `store`.
    async fn seed(&mut self, store: &impl StoreBackend, plugin: &str, value: String) {
        let unit = UnitRecord {
            unit: self.minter.mint().expect("a unit"),
            meta: UnitMeta {
                realm: self.realm.clone(),
                principal: self.principal.clone(),
                kind: UnitKind::Step,
                label: "seeding how the plugin activates".to_owned(),
                committed_at_ms: 0,
            },
            writes: vec![StagedWrite {
                plugin: plugin.to_owned(),
                key: ACTIVATION.to_owned(),
                expect: Expect::Absent,
                after: Some(value),
            }],
            blobs: Vec::new(),
            notes: Vec::new(),
            unpinned: Vec::new(),
            marks: Vec::new(),
        };
        let quotas = Quotas {
            entries_per_plugin: 10,
            lowered: std::collections::BTreeMap::new(),
            blobs: 10,
            writes_per_unit: 10,
            notes_per_unit: 10,
        };
        store
            .commit(&unit, &quotas)
            .await
            .expect("the seed commits");
    }
}

/// A kernel over `store`, deciding with `pdp`, with a journal, a directory and
/// a log of its own that nothing here reads: a load writes none of them.
async fn kernel_deciding_with<E>(store: E::Store, pdp: Result<Pdp, nc_authz::PdpError>) -> Kernel<E>
where
    E: Environment<
            Blobs = nc_kernel::blobs::MemoryBlobs,
            Journal = nc_journal::InMemory,
            Clock = SystemClock,
            Directory = MemoryDirectory,
            Log = nc_kernel::logging::Captured,
            Transport = nc_kernel::net::Scripted,
        >,
{
    let family = Pdp::embedded("family").expect("the family pack is carried");
    let directory = MemoryDirectory::from_world(WORLD, &family).expect("the world is valid");
    let realm = directory.realm_id("home").expect("home is a realm");
    let keeper = OpaqueId::new("kernel").expect("a keeper"); // allow-logging: OpaqueId::new of a literal this suite writes
    Kernel::open(KernelConfig {
        store,
        blobs: nc_kernel::blobs::MemoryBlobs::default(),
        pdp,
        journal: nc_journal::Journal::new(
            nc_journal::InMemory::default(),
            realm,
            keeper,
            Minter::new(MachineFlake::from_value(2).expect("a flake"), SystemClock),
        ),
        directory,
        clock: SystemClock,
        log: nc_kernel::logging::Captured::default(),
        transport: nc_kernel::net::Scripted::default(),
    })
    .await
    .expect("the kernel opens")
}

/// A kernel over `store`, deciding with the family pack.
async fn kernel<E>(store: E::Store) -> Kernel<E>
where
    E: Environment<
            Blobs = nc_kernel::blobs::MemoryBlobs,
            Journal = nc_journal::InMemory,
            Clock = SystemClock,
            Directory = MemoryDirectory,
            Log = nc_kernel::logging::Captured,
            Transport = nc_kernel::net::Scripted,
        >,
{
    kernel_deciding_with(store, Pdp::embedded("family")).await
}

/// The reference plugin's component, as `cargo xtask build --plugins` wrote it.
fn component() -> Vec<u8> {
    nc_kernel::component_for_tests("reference")
}

/// The codes and sentences of a refusal.
fn problems(refused: &LoadError) -> Vec<(ErrorCode, String)> {
    refused
        .problems()
        .iter()
        .map(|problem| (problem.code(), problem.sentence().to_owned()))
        .collect()
}

/// Loads the reference plugin activating as `variant` under `reference`, into
/// a kernel of its own, and answers what the refusal said — after holding the
/// refusal to leaving nothing of the plugin registered.
async fn refused_as(variant: Variant) -> Vec<(ErrorCode, String)> {
    let store = MemoryBackend::default();
    let activation = Activation {
        namespace: "reference".to_owned(),
        variant,
        subscribe: Vec::new(),
    };
    Seeder::new()
        .seed(&store, "noctua:reference", activation.written())
        .await;
    let kernel = kernel::<InMemory>(store).await;
    let refused = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect_err("the plugin declares a command the kernel refuses");
    assert!(
        kernel
            .registry()
            .iter()
            .all(|command| command.owner == Owner::Kernel),
        "a refused plugin left commands registered: {:?}",
        kernel.registry()
    );
    problems(&refused)
}

#[tokio::test]
async fn a_plugin_activating_as_written_registers_every_command_it_declares() {
    let store = MemoryBackend::default();
    let activation = Activation {
        namespace: "reference".to_owned(),
        variant: Variant::Normal,
        subscribe: Vec::new(),
    };
    Seeder::new()
        .seed(&store, "noctua:reference", activation.written())
        .await;
    let kernel = kernel::<InMemory>(store).await;
    let slot = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect("every command the plugin declares is admitted");
    assert_eq!(slot.index(), 0, "the first plugin loaded");
    let registered: Vec<_> = kernel
        .registry()
        .into_iter()
        .filter(|command| command.owner == Owner::Plugin(slot))
        .collect();
    let mut declared = commands_for(&activation);
    declared.sort_by(|one, other| one.name.cmp(&other.name));
    assert_eq!(
        registered
            .iter()
            .map(|command| command.name.clone())
            .collect::<Vec<_>>(),
        written_under("reference")
    );
    assert!(registered.len() > 2, "{registered:?}");
    for (registered, declared) in registered.iter().zip(&declared) {
        assert_eq!(registered.name, declared.name);
        assert_eq!(registered.title, declared.title);
        assert_eq!(registered.args_schema, declared.args_schema);
        assert_eq!(registered.action, declared.action);
        assert_eq!(registered.resource_selector, declared.resource_selector);
        assert_eq!(registered.mutating, declared.mutating);
        // The field the decision point's irreversible precondition reads: a
        // registry that lists every command as a read passes every assertion
        // above, measured with `reference.probe` (declared `write`) listed so.
        assert_eq!(
            registered.reversibility,
            match declared.reversibility {
                Reversibility::Read => nc_schema::descriptor::Reversibility::Read,
                Reversibility::Write => nc_schema::descriptor::Reversibility::Write,
                Reversibility::Irreversible => nc_schema::descriptor::Reversibility::Irreversible,
            },
            "`{}`",
            registered.name
        );
    }
    let own: Vec<_> = kernel
        .registry()
        .into_iter()
        .filter(|command| command.owner == Owner::Kernel)
        .map(|command| command.name)
        .collect();
    assert!(
        own.contains(&"journal.verify".to_owned()) && own.contains(&"limits.prune".to_owned()),
        "the kernel's own commands are registered beside the plugin's: {own:?}"
    );
}

/// A plugin whose manifest does not grant it its store reads nothing of it, and
/// activates as written.
///
/// **Over a store holding an activation that breaks its first command.** Over
/// an empty store a read finds nothing whether it was granted or not, so the
/// plugin would activate as written either way, and a load that granted every
/// plugin every capability, whatever its manifest said, would pass such a test:
/// `Kernel::load` handed every grant, which the operator
/// `a plugin is granted no more than its manifest names` in `xtask/mutants.py`
/// plants.
#[tokio::test]
async fn a_plugin_not_granted_its_store_activates_as_written_under_its_own_namespace() {
    let store = MemoryBackend::default();
    Seeder::new()
        .seed(
            &store,
            "noctua:reference",
            Activation {
                namespace: "reference".to_owned(),
                variant: Variant::MissingAction,
                subscribe: Vec::new(),
            }
            .written(),
        )
        .await;
    let kernel = kernel::<InMemory>(store).await;
    let ungranted = PluginManifest::parse(
        "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = []\n",
    )
    .expect("a manifest the kernel reads");
    let slot = kernel
        .load(ungranted, &component())
        .await
        .expect("the table as written is admitted, the seed being unread");
    let names: Vec<String> = kernel
        .registry()
        .into_iter()
        .filter(|command| command.owner == Owner::Plugin(slot))
        .map(|command| command.name)
        .collect();
    assert_eq!(names, written_under("reference"));
}

#[tokio::test]
async fn a_descriptor_missing_its_action_is_rejected_at_activate_naming_the_command() {
    assert_eq!(
        refused_as(Variant::MissingAction).await,
        [(
            ErrorCode::PluginDescriptorMissingField,
            "`reference.note.get` of `noctua:reference` declares no action".to_owned()
        )]
    );
}

#[tokio::test]
async fn a_descriptor_missing_its_resource_selector_is_rejected_at_activate_naming_the_command() {
    assert_eq!(
        refused_as(Variant::MissingSelector).await,
        [(
            ErrorCode::PluginDescriptorMissingField,
            "`reference.note.get` of `noctua:reference` declares no resource selector".to_owned()
        )]
    );
}

#[tokio::test]
async fn a_descriptor_missing_its_name_is_rejected_at_activate_naming_its_position() {
    assert_eq!(
        refused_as(Variant::EmptyName).await,
        [(
            ErrorCode::PluginDescriptorMissingField,
            "the command at position 1 of `noctua:reference` declares no name".to_owned()
        )]
    );
}

#[tokio::test]
async fn a_descriptor_missing_its_title_is_rejected_at_activate_naming_the_command() {
    assert_eq!(
        refused_as(Variant::EmptyTitle).await,
        [(
            ErrorCode::PluginDescriptorMissingField,
            "`reference.note.get` of `noctua:reference` declares no title".to_owned()
        )]
    );
}

#[tokio::test]
async fn a_descriptor_missing_its_args_schema_is_rejected_at_activate_naming_the_command() {
    assert_eq!(
        refused_as(Variant::EmptySchema).await,
        [(
            ErrorCode::PluginDescriptorMissingField,
            "`reference.note.get` of `noctua:reference` declares no argument schema".to_owned()
        )]
    );
}

/// The code and the sentence of the one problem `variant` was refused for.
async fn the_one_problem(variant: Variant) -> (ErrorCode, String) {
    let mut problems = refused_as(variant).await;
    assert_eq!(problems.len(), 1, "{variant:?}: {problems:?}");
    problems.remove(0)
}

#[tokio::test]
async fn a_descriptor_with_an_unknown_schema_keyword_is_rejected() {
    let (code, sentence) = the_one_problem(Variant::UnknownKeyword).await;
    assert_eq!(code, ErrorCode::PluginDescriptorInvalid);
    assert!(
        sentence.starts_with("`reference.note.get` of `noctua:reference` has an argument schema")
            && sentence.contains("pattern"),
        "{sentence}"
    );
}

#[tokio::test]
async fn a_selector_naming_no_required_entity_property_is_rejected() {
    let (code, sentence) = the_one_problem(Variant::BadSelector).await;
    assert_eq!(code, ErrorCode::PluginDescriptorInvalid);
    assert!(
        sentence.starts_with("`reference.note.get` of `noctua:reference` has a resource selector")
            && sentence.contains("nothing"),
        "{sentence}"
    );
}

#[tokio::test]
async fn a_read_that_mutates_is_rejected() {
    let (code, sentence) = the_one_problem(Variant::ReadThatMutates).await;
    assert_eq!(code, ErrorCode::PluginDescriptorInvalid);
    assert!(
        sentence.starts_with(
            "`reference.note.get` of `noctua:reference` is declared `read` and mutating"
        ),
        "{sentence}"
    );
}

#[tokio::test]
async fn a_command_outside_the_plugins_namespace_or_in_a_reserved_one_is_rejected() {
    let (code, sentence) = the_one_problem(Variant::OutsideNamespace).await;
    assert_eq!(code, ErrorCode::KernelNameTaken);
    assert!(
        sentence.starts_with(
            "`reference-elsewhere.note.get` of `noctua:reference` is outside its namespace"
        ),
        "{sentence}"
    );
    let (code, sentence) = the_one_problem(Variant::ReservedNamespace).await;
    assert_eq!(code, ErrorCode::KernelNameTaken);
    assert!(
        sentence.starts_with(
            "`kernel.note.get` of `noctua:reference` is in the namespace `kernel.`, which the \
             kernel keeps"
        ),
        "{sentence}"
    );
}

/// A command selecting a feature of the platform is refused, and named.
///
/// **Through the real component**: the judge every built-in is held to
/// accepts the form, so the kernel's own refusal at `activate` is what keeps a
/// plugin from borrowing the permit a person's naming commands are decided by.
#[tokio::test]
async fn a_command_selecting_a_feature_of_the_platform_is_refused_and_named() {
    let (code, sentence) = the_one_problem(Variant::SelectsAFeature).await;
    assert_eq!(code, ErrorCode::PluginDescriptorInvalid);
    assert_eq!(
        sentence,
        "`reference.note.get` of `noctua:reference` selects the feature `naming` of the \
         platform, which only a command of the kernel's own selects"
    );
}

#[tokio::test]
async fn an_action_the_schema_does_not_declare_is_rejected() {
    let (code, sentence) = the_one_problem(Variant::UnknownAction).await;
    assert_eq!(code, ErrorCode::PluginDescriptorInvalid);
    assert!(
        sentence
            .starts_with("`reference.note.get` of `noctua:reference` names the action `Rename`"),
        "{sentence}"
    );
}

#[tokio::test]
async fn a_plugin_that_traps_while_activating_is_not_loaded() {
    let (code, sentence) = the_one_problem(Variant::Trap).await;
    assert_eq!(code, ErrorCode::PluginActivationFailed);
    assert!(
        sentence.starts_with("`noctua:reference` did not activate: ")
            && sentence.contains("unreachable"),
        "{sentence}"
    );
}

#[tokio::test]
async fn a_plugin_refusing_to_activate_is_not_loaded_and_says_why() {
    let store = MemoryBackend::default();
    Seeder::new()
        .seed(
            &store,
            "noctua:reference",
            r#"{"namespace":"reference","variant":"sideways"}"#.to_owned(),
        )
        .await;
    let kernel = kernel::<InMemory>(store).await;
    let refused = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect_err("the plugin cannot read its activation");
    let problems = problems(&refused);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].0, ErrorCode::PluginActivationFailed);
    assert!(
        problems[0].1.starts_with(
            "`noctua:reference` refused to activate, answering `invalid` with `the store"
        ) && problems[0].1.contains("no variant is `sideways``"),
        "{}",
        problems[0].1
    );
}

/// A plugin's own refusal is repeated cut to a bound, and what a reader cannot
/// see in it is escaped.
///
/// **Bounded, as what the registry repeats is.** Repeated whole, the refusal of
/// a plugin seeded to refuse with a variant of a million characters would be a
/// sentence of 1,000,157, its direction override escaped and nothing else
/// bounded, while the registry holds every action, entity type and topic it
/// repeats to sixty-four, and repeats a command's name only when it is well
/// formed.
#[tokio::test]
async fn a_plugins_own_refusal_is_repeated_cut_to_a_bound_and_escaped() {
    let store = MemoryBackend::default();
    let variant = format!("\u{202e}{}", "x".repeat(10_000));
    Seeder::new()
        .seed(
            &store,
            "noctua:reference",
            nc_canon::Value::object([
                ("namespace", nc_canon::Value::text("reference".to_owned())),
                ("variant", nc_canon::Value::text(variant)),
            ])
            .to_canonical(),
        )
        .await;
    let kernel = kernel::<InMemory>(store).await;
    let refused = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect_err("the plugin cannot read its activation");
    let problems = problems(&refused);
    assert_eq!(problems.len(), 1, "{problems:?}");
    let (code, sentence) = &problems[0];
    assert_eq!(*code, ErrorCode::PluginActivationFailed);
    assert!(
        sentence.starts_with(
            "`noctua:reference` refused to activate, answering `invalid` with `the store"
        ) && sentence.contains("no variant is `\\u{202e}xxx")
            && !sentence.contains('\u{202e}'),
        "the override is shown as its escape and never as itself: {sentence}"
    );
    // The plugin's first 256 characters, two of them escaped, and the kernel's
    // own words around them: the 10,000 the plugin wrote are not repeated.
    assert!(
        sentence.ends_with("xxx…`") && sentence.chars().count() < 400,
        "{} characters, ending {:?}",
        sentence.chars().count(),
        sentence.chars().rev().take(16).collect::<String>()
    );
}

#[tokio::test]
async fn a_plugin_whose_memory_is_lowered_below_what_it_starts_with_is_not_loaded() {
    let kernel = kernel::<InMemory>(MemoryBackend::default()).await;
    let small = PluginManifest::parse(
        "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = []\n\
         [limits]\nmemory_bytes = 65536\n",
    )
    .expect("a lowering within the catalog");
    let refused = kernel
        .load(small, &component())
        .await
        .expect_err("the plugin's memory starts larger than one page");
    let problems = problems(&refused);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].0, ErrorCode::PluginActivationFailed);
    assert!(
        problems[0].1.contains("its memory asked to grow to")
            && problems[0]
                .1
                .contains("past the 65536 its `memory_bytes` allows"),
        "{}",
        problems[0].1
    );
}

#[tokio::test]
async fn bytes_that_are_not_a_component_of_the_world_are_not_loaded() {
    let kernel = kernel::<InMemory>(MemoryBackend::default()).await;
    for (bytes, says) in [
        (
            b"not a component".to_vec(),
            "is not a component this kernel compiles",
        ),
        (
            b"\0asm\x0d\0\x01\0".to_vec(),
            "does not link against the world `plugin`",
        ),
    ] {
        let refused = kernel
            .load(manifest("noctua:reference", "reference"), &bytes)
            .await
            .expect_err("not a plugin");
        let problems = problems(&refused);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].0, ErrorCode::PluginComponentInvalid);
        assert!(
            problems[0]
                .1
                .starts_with(&format!("`noctua:reference` {says}")),
            "{}",
            problems[0].1
        );
    }
}

/// What wasmtime says of bytes it will not compile or link is repeated cut to
/// a bound.
///
/// **The component's own names, bounded.** wasmtime's sentence quotes the
/// component's own names, which its author wrote, and repeated whole, a
/// component importing a function named with 100,000 characters would be
/// refused in a sentence of 100,185. The test above holds only how each
/// sentence opens.
#[tokio::test]
async fn what_wasmtime_says_of_a_component_it_refuses_is_repeated_cut_to_a_bound() {
    let kernel = kernel::<InMemory>(MemoryBackend::default()).await;
    let long = "x".repeat(10_000);
    for (bytes, says) in [
        (
            a_component_whose_import_is_named(&format!("{long}_")),
            "is not a component this kernel compiles: `wasmtime refused the component: ",
        ),
        (
            a_component_importing(&long),
            "does not link against the world `plugin`: `",
        ),
    ] {
        let refused = kernel
            .load(manifest("noctua:reference", "reference"), &bytes)
            .await
            .expect_err("not a plugin");
        let problems = problems(&refused);
        assert_eq!(problems.len(), 1, "{} problems", problems.len());
        let (code, sentence) = &problems[0];
        assert_eq!(*code, ErrorCode::PluginComponentInvalid);
        // The name reached the sentence, or a bound on it would hold of a
        // sentence that never repeated it.
        assert!(
            sentence.starts_with(&format!("`noctua:reference` {says}"))
                && sentence.contains("xxxx")
                && sentence.ends_with("…`")
                && sentence.chars().count() < 1_000,
            "{} characters, opening {:?}",
            sentence.chars().count(),
            sentence.chars().take(160).collect::<String>()
        );
    }
}

/// A component, byte for byte, whose one import is a function named `name`:
/// one taking nothing and returning nothing, written out rather than encoded
/// so the name can be one wasmtime refuses to compile.
fn a_component_whose_import_is_named(name: &str) -> Vec<u8> {
    let section = |id: u8, content: &[u8]| [&[id][..], &sized(content)].concat();
    [
        // The header: the component model's version and layer.
        b"\0asm\x0d\0\x01\0".to_vec(),
        // The type section: one function type, nothing in and nothing out.
        section(7, &[1, 0x40, 0x00, 0x01, 0x00]),
        // The import section: `name`, a function of that type.
        section(
            10,
            &[&[1, 0x00][..], &sized(name.as_bytes()), &[0x01, 0x00]].concat(),
        ),
    ]
    .concat()
}

/// The smallest component that imports one function by itself, named `name`,
/// taking nothing and returning nothing: one that compiles, and that no world
/// the kernel hosts defines.
fn a_component_importing(name: &str) -> Vec<u8> {
    let section = |id: u8, content: &[u8]| [&[id][..], &sized(content)].concat();
    let mut module = [
        b"\0asm\x01\0\0\0".to_vec(),
        // The type section: one function type, nothing in and nothing out.
        section(1, &[1, 0x60, 0, 0]),
        // The import section: that type, imported as the root's `name`.
        section(
            2,
            &[&[1][..], &sized(b"$root"), &sized(name.as_bytes()), &[0, 0]].concat(),
        ),
        // The memory section: one memory of one page and no maximum.
        section(5, &[1, 0, 1]),
        // The export section: that memory, as `memory`.
        section(7, &[&[1][..], &sized(b"memory"), &[2, 0]].concat()),
    ]
    .concat();
    let mut resolve = wit_parser::Resolve::default();
    let probe = resolve
        .push_str(
            "probe.wit",
            &format!("package nc:probe;\n\nworld probe {{\n  import {name}: func();\n}}\n"),
        )
        .expect("the probe's world parses");
    let world = resolve
        .select_world(&[probe], Some("probe"))
        .expect("the probe's world is there");
    wit_component::embed_component_metadata(
        &mut module,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
    )
    .expect("the world embeds in the module");
    wit_component::ComponentEncoder::default()
        .module(&module)
        .expect("the module carries the world it imports from")
        .validate(true)
        .encode()
        .expect("the module encodes as a component")
}

/// `bytes`, after their length as LEB128 writes it: seven bits a byte, the
/// high bit set on every byte but the last — so a name past 127 bytes is
/// written as WebAssembly writes it.
fn sized(bytes: &[u8]) -> Vec<u8> {
    let mut length = bytes.len();
    let mut written = Vec::with_capacity(bytes.len() + 5);
    loop {
        let low = u8::try_from(length & 0x7f).expect("seven bits fit in a byte");
        length >>= 7;
        if length == 0 {
            written.push(low);
            break;
        }
        written.push(low | 0x80);
    }
    written.extend_from_slice(bytes);
    written
}

#[tokio::test]
async fn a_namespace_the_kernel_keeps_is_refused_before_anything_is_compiled() {
    let kernel = kernel::<InMemory>(MemoryBackend::default()).await;
    for namespace in ["journal", "kernel", "limits", "naming"] {
        let refused = kernel
            .load(
                manifest("noctua:reference", namespace),
                b"never compiled, because the namespace is refused first",
            )
            .await
            .expect_err("a namespace the kernel keeps");
        assert_eq!(
            problems(&refused),
            [(
                ErrorCode::PluginIdentityTaken,
                format!(
                    "`noctua:reference` asks for the namespace `{namespace}.`, which the \
                     kernel keeps for its own"
                )
            )]
        );
    }
}

#[tokio::test]
async fn two_plugins_load_side_by_side_and_neither_identity_nor_namespace_is_taken_twice() {
    let store = MemoryBackend::default();
    let mut seeder = Seeder::new();
    let peer = Activation {
        namespace: "peer".to_owned(),
        variant: Variant::Normal,
        subscribe: Vec::new(),
    };
    seeder
        .seed(&store, "noctua:reference-peer", peer.written())
        .await;
    let kernel = kernel::<InMemory>(store).await;
    let first = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect("the first plugin loads");
    let second = kernel
        .load(manifest("noctua:reference-peer", "peer"), &component())
        .await
        .expect("the same component loads again as another plugin");
    assert_ne!(first, second);
    let owned_by = |slot| {
        kernel
            .registry()
            .into_iter()
            .filter(|command| command.owner == Owner::Plugin(slot))
            .map(|command| command.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(owned_by(first), written_under("reference"));
    assert_eq!(owned_by(second), written_under("peer"));
    for (again, says) in [
        (
            manifest("noctua:reference", "other"),
            "`noctua:reference` is loaded already".to_owned(),
        ),
        (
            manifest("noctua:third", "peer"),
            "`noctua:third` asks for the namespace `peer.`, which `noctua:reference-peer` holds"
                .to_owned(),
        ),
    ] {
        let refused = kernel.load(again, &component()).await.expect_err("taken");
        assert_eq!(problems(&refused), [(ErrorCode::PluginIdentityTaken, says)]);
    }
}

#[tokio::test]
async fn no_plugin_is_loaded_while_there_is_no_decision_point_to_judge_it_against() {
    let not_carried = Pdp::embedded("collective").expect_err("no pack for this profile");
    let code = not_carried.code();
    let kernel = kernel_deciding_with::<InMemory>(MemoryBackend::default(), Err(not_carried)).await;
    let refused = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect_err("nothing to judge the commands against");
    let problems = problems(&refused);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].0, code);
    assert!(
        problems[0].1.starts_with(
            "`noctua:reference` was not loaded, because the decision point its commands are \
             judged against could not be built"
        ),
        "{}",
        problems[0].1
    );
}

#[tokio::test]
async fn a_plugin_reads_its_activation_from_the_local_database() {
    let store = nc_local::LocalStore::in_memory_for_tests()
        .await
        .expect("the local database opens in memory");
    let activation = Activation {
        namespace: "reference".to_owned(),
        variant: Variant::MissingAction,
        subscribe: Vec::new(),
    };
    Seeder::new()
        .seed(&store, "noctua:reference", activation.written())
        .await;
    let kernel = kernel::<InTheLocalDatabase>(store).await;
    let refused = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect_err("the activation the file holds breaks the first command");
    assert_eq!(
        problems(&refused),
        [(
            ErrorCode::PluginDescriptorMissingField,
            "`reference.note.get` of `noctua:reference` declares no action".to_owned()
        )]
    );
}

#[tokio::test]
async fn a_refusal_reads_as_every_problem_each_with_its_code() {
    let store = MemoryBackend::default();
    Seeder::new()
        .seed(
            &store,
            "noctua:reference",
            Activation {
                namespace: "reference".to_owned(),
                variant: Variant::MissingAction,
                subscribe: Vec::new(),
            }
            .written(),
        )
        .await;
    let kernel = kernel::<InMemory>(store).await;
    let refused = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect_err("refused");
    assert_eq!(
        refused.to_string(),
        format!(
            "[{}] `reference.note.get` of `noctua:reference` declares no action",
            ErrorCode::PluginDescriptorMissingField
        )
    );
}

/// A store over [`MemoryBackend`] whose reads first let every other task run,
/// and fail when it is told to.
///
/// **What makes a load wait in the middle of `activate`.** A read of the store
/// held in memory is ready at once, so one load runs from its first check to
/// its registration without letting another in; this one yields first, as a
/// read of a file does, so two loads interleave the way a daemon's would.
#[derive(Debug, Default)]
struct Waiting {
    inner: MemoryBackend,
    failing: bool,
}

/// A kernel over a [`Waiting`] store.
#[derive(Debug)]
struct OverAWaitingStore;

impl Environment for OverAWaitingStore {
    type Store = Waiting;
    type Blobs = nc_kernel::blobs::MemoryBlobs;
    type Journal = nc_journal::InMemory;
    type Clock = SystemClock;
    type Directory = MemoryDirectory;
    type Log = nc_kernel::logging::Captured;
    type Transport = nc_kernel::net::Scripted;
}

impl StoreBackend for Waiting {
    async fn get(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
        key: &str,
    ) -> Result<Option<Entry>, LocalError> {
        tokio::task::yield_now().await;
        if self.failing {
            return Err(LocalError::Unavailable(
                "the disk did not answer".to_owned(),
            ));
        }
        self.inner.get(realm, plugin, key).await
    }

    fn list(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
        prefix: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> impl Future<Output = Result<Page, LocalError>> + Send {
        self.inner.list(realm, plugin, prefix, limit, cursor)
    }

    fn count(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        self.inner.count(realm, plugin)
    }

    fn commit(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<Committed, CommitError>> + Send {
        self.inner.commit(unit, quotas)
    }

    fn commit_held(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<Committed, CommitError>> + Send {
        self.inner.commit_held(unit, quotas)
    }

    fn release(
        &self,
        holder: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        self.inner.release(holder)
    }

    fn holders(&self) -> impl Future<Output = Result<Vec<nc_ids::Snowflake>, LocalError>> + Send {
        self.inner.holders()
    }

    fn units(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<UnitRow>, LocalError>> + Send {
        self.inner.units(realm, principal, limit)
    }

    fn steps(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<UnitRow>, LocalError>> + Send {
        self.inner.steps(realm, principal, limit)
    }

    fn unit(
        &self,
        unit: Snowflake,
    ) -> impl Future<Output = Result<Option<UnitDetail>, LocalError>> + Send {
        self.inner.unit(unit)
    }

    fn set_state(
        &self,
        unit: Snowflake,
        state: UnitState,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        self.inner.set_state(unit, state)
    }

    fn forget_beyond(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        keep: u32,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        self.inner.forget_beyond(realm, principal, keep)
    }

    fn noted_elsewhere(
        &self,
        note: &str,
        except: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        self.inner.noted_elsewhere(note, except)
    }

    fn blob(&self, hash: &str) -> impl Future<Output = Result<Option<BlobRow>, LocalError>> + Send {
        self.inner.blob(hash)
    }

    fn set_pinned(
        &self,
        hash: &str,
        pinned: bool,
        at_ms: u64,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        self.inner.set_pinned(hash, pinned, at_ms)
    }

    fn unpinned_before(
        &self,
        before_ms: u64,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<String>, LocalError>> + Send {
        self.inner.unpinned_before(before_ms, limit)
    }

    fn forget_blob(&self, hash: &str) -> impl Future<Output = Result<bool, LocalError>> + Send {
        self.inner.forget_blob(hash)
    }

    fn pruner(&self) -> Option<nc_prune::sqlite::Sqlite> {
        self.inner.pruner()
    }
}

#[tokio::test]
async fn a_plugin_granted_a_store_holding_no_activation_activates_as_written() {
    let kernel = kernel::<InMemory>(MemoryBackend::default()).await;
    let slot = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect("the table as written is admitted");
    let names: Vec<String> = kernel
        .registry()
        .into_iter()
        .filter(|command| command.owner == Owner::Plugin(slot))
        .map(|command| command.name)
        .collect();
    assert_eq!(names, written_under("reference"));
}

#[tokio::test]
async fn a_plugin_whose_store_does_not_answer_refuses_to_activate() {
    let kernel = kernel::<OverAWaitingStore>(Waiting {
        inner: MemoryBackend::default(),
        failing: true,
    })
    .await;
    let refused = kernel
        .load(manifest("noctua:reference", "reference"), &component())
        .await
        .expect_err("the plugin cannot read how to activate");
    let problems = problems(&refused);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].0, ErrorCode::PluginActivationFailed);
    assert!(
        problems[0]
            .1
            .starts_with("`noctua:reference` refused to activate, answering `unavailable` with `")
            && problems[0].1.contains("the disk did not answer"),
        "{}",
        problems[0].1
    );
}

/// Two loads of one plugin at once: the one that finishes first is admitted,
/// and the other is refused by the check it makes again before registering.
///
/// **The first check alone let both through.** Each load asks whether the
/// identity is free before it compiles, and both ask before either has
/// registered — so without asking again under the lock, after `activate`, one
/// identity would be loaded twice, two stores reading the same keys. Which of
/// the two finishes first is the scheduler's to say, and either way exactly one
/// is admitted.
#[tokio::test]
async fn a_load_that_finishes_first_takes_the_identity_from_one_still_activating() {
    let kernel = kernel::<OverAWaitingStore>(Waiting::default()).await;
    let component = component();
    let (one, other) = tokio::join!(
        kernel.load(manifest("noctua:reference", "reference"), &component),
        kernel.load(manifest("noctua:reference", "reference"), &component),
    );
    let (slot, refused) = match (one, other) {
        (Ok(slot), Err(refused)) | (Err(refused), Ok(slot)) => (slot, refused),
        (one, other) => panic!("one load is admitted and the other refused: {one:?}, {other:?}"),
    };
    assert_eq!(
        problems(&refused),
        [(
            ErrorCode::PluginIdentityTaken,
            "`noctua:reference` is loaded already — by a load that finished while this \
             one activated"
                .to_owned()
        )]
    );
    assert_eq!(
        kernel
            .registry()
            .iter()
            .filter(|command| command.owner != Owner::Kernel)
            .map(|command| command.owner)
            .collect::<Vec<_>>(),
        vec![Owner::Plugin(slot); written_under("reference").len()],
        "one plugin's commands, and nothing of the other load"
    );
}

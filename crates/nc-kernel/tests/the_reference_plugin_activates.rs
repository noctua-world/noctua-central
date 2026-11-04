//! The reference plugin, built for `wasm32` and encoded as a component, runs in
//! this kernel.
//!
//! The first thing Stage 11 can show end to end: `plugins/reference`, compiled
//! by `cargo xtask build --plugins` against `crates/nc-guest`'s half of the
//! world, is compiled here by the kernel's engine against the other half and
//! asked to activate. That the link succeeds is the proof the two halves are
//! one world; that it answers with its commands is the proof the call crossed
//! the boundary and came back.

/// The reference plugin activates and declares its commands, every field of
/// each as the plugin's own table writes it.
///
/// **Field for field, not by name.** Compared by the names alone, a component
/// whose adapter declared the read as mutating, under an empty action and with
/// the other reversibility, would activate and pass — measured, with the three
/// planted in `plugins/reference/src/adapter.rs` and the component built.
/// The adapter is compiled for `wasm32` only, so no host test reads it; what it
/// produces is read here, against `behaviour`, which the plugin's own host
/// tests hold to the judge.
#[tokio::test]
async fn the_reference_plugin_activates_and_declares_its_commands() {
    use nc_plugin_reference::behaviour::{Reversibility, commands};

    let engine = nc_kernel::engine::Engine::new().expect("the engine's configuration is valid");
    let compiled = engine
        .compile(&nc_kernel::component_for_tests("reference"))
        .expect("the component the build wrote compiles");
    let declared = nc_kernel::activate_for_tests(&engine, &compiled)
        .await
        .expect("the reference plugin activates, granted nothing, without reaching for the host");
    let written: Vec<nc_kernel::DeclaredForTests> = commands()
        .into_iter()
        .map(|command| nc_kernel::DeclaredForTests {
            name: command.name,
            title: command.title.to_owned(),
            args_schema: command.args_schema.to_owned(),
            mutating: command.mutating,
            action: command.action.to_owned(),
            resource_selector: command.resource_selector.to_owned(),
            reversibility: match command.reversibility {
                Reversibility::Read => "read",
                Reversibility::Write => "write",
                Reversibility::Irreversible => "irreversible",
            },
        })
        .collect();
    assert_eq!(
        declared, written,
        "the component declares something other than the plugin's own table: the \
         adapter maps a descriptor wrongly, or the component is not the one its \
         sources build"
    );
    // And the commands by name, so a table that emptied on both sides is not
    // two empty lists agreeing.
    let named: Vec<&str> = declared
        .iter()
        .map(|command| command.name.as_str())
        .collect();
    assert_eq!(named[..2], ["reference.note.get", "reference.probe"]);
    assert!(
        named.contains(&"reference.irreversible.do") && named.contains(&"reference.relay.write"),
        "{named:?}"
    );
}

/// A plugin imports what it calls, and the reference plugin calls everything.
///
/// Its probe calls every function of the world, so its component imports each
/// of them — as the component model names an import, the interface's whole
/// name and the function — and only those. `plugins/reference/plugin.toml`
/// grants it nothing: what a plugin imports is what it can reach for, and its
/// grants are what it is answered, which `crates/nc-kernel/tests/capabilities.rs`
/// holds.
#[test]
fn the_reference_plugin_imports_every_function_it_probes_and_nothing_else() {
    let engine = nc_kernel::engine::Engine::new().expect("the engine's configuration is valid");
    let compiled = engine
        .compile(&nc_kernel::component_for_tests("reference"))
        .expect("the component the build wrote compiles");
    let mut imported = compiled.imports(&engine);
    imported.sort();
    let mut probed: Vec<String> = nc_plugin_reference::behaviour::PROBED
        .iter()
        .map(|function| {
            let (interface, name) = function.split_once('#').expect("interface#function");
            format!(
                "noctua:kernel/{interface}@{}#{name}",
                nc_kernel::world::WORLD_VERSION
            )
        })
        .collect();
    probed.sort();
    assert_eq!(imported, probed);
}

/// A component that calls one host function imports exactly that one.
///
/// **A control beside the test above.** The reference plugin imports every
/// function, so a walk that answered every function of the world whatever a
/// component imports would pass that test, as an import walk that can never
/// find anything — the filter in `Compiled::imports` changed from a component
/// function to a core function, which an instance never exports — passes any
/// assertion that the answer is empty. So beside that plugin stands a component
/// that imports one thing, and the walk has to name that one alone.
///
/// Built here rather than as a plugin, because the reference plugin imports
/// every function and a control has to import exactly one: a core module whose
/// one import is `write` from the world's `log` interface, carrying a world that
/// imports that interface and exports nothing, encoded the way `cargo xtask
/// build --plugins` encodes a plugin.
#[test]
fn a_component_that_calls_one_host_function_imports_that_one() {
    let engine = nc_kernel::engine::Engine::new().expect("the engine's configuration is valid");
    let compiled = engine
        .compile(&one_call_to_the_log())
        .expect("the probe compiles");
    assert_eq!(compiled.imports(&engine), ["noctua:kernel/log@0.2.0#write"]);
}

/// An import of the world is linked to the host, not refused at the link.
///
/// **Whether the link takes the import at all**: a link that stubbed every
/// import, as a trap or as a no-op, would pass this too, so what it asks is
/// whether the import is linked rather than refused. This probe imports
/// `log.write` and exports no `guest`, so how far it gets is the answer: with
/// the host linked the link succeeds and the refusal is the missing export;
/// without it the link itself refuses the `log` import, and the export is
/// never looked for.
#[tokio::test]
async fn an_import_of_the_world_is_linked_rather_than_refused_at_the_link() {
    let engine = nc_kernel::engine::Engine::new().expect("the engine's configuration is valid");
    let compiled = engine
        .compile(&one_call_to_the_log())
        .expect("the probe compiles");
    let refused = nc_kernel::activate_for_tests(&engine, &compiled)
        .await
        .expect_err("a component that exports no guest is not a plugin");
    assert!(
        refused.contains("noctua:kernel/guest") && !refused.contains("noctua:kernel/log"),
        "the refusal is the missing export, not the import the link should have stubbed: \
         {refused}"
    );
}

/// A component that imports a function by itself, outside any interface,
/// lists it.
///
/// **The reason the walk takes every import.** The kernel compiles whatever
/// bytes it is handed, and this probe — a world whose one import
/// is `foo: func()`, encoded the way `cargo xtask build --plugins` encodes a
/// plugin — compiles, and a walk that skipped every import that is not an
/// instance would report an empty surface for it. Where a link stubs `foo` like
/// any other import, the surface a manifest's grants have to cover is one
/// function wider than such a walk answers; the host refuses `foo` at the link,
/// and the walk still names it, because what a component imports is what it can
/// reach for.
#[test]
fn a_component_that_imports_a_bare_function_lists_it() {
    let engine = nc_kernel::engine::Engine::new().expect("the engine's configuration is valid");
    let compiled = engine
        .compile(&one_bare_function())
        .expect("the probe compiles");
    assert_eq!(
        compiled.imports(&engine),
        ["foo"],
        "the component imports the host function `foo` and the walk did not list it"
    );
}

/// A function inside an instance inside an instance is listed by its whole path.
///
/// WIT cannot write this shape — an interface holds no interface — and the
/// component model can, so a component built by hand can import it, and the
/// link refuses it because the world defines nothing there; a walk that
/// stopped one level down would report this component as reaching nothing.
#[test]
fn a_function_inside_an_instance_inside_an_instance_is_listed_by_its_path() {
    let engine = nc_kernel::engine::Engine::new().expect("the engine's configuration is valid");
    let compiled = engine
        .compile(&an_instance_inside_an_instance())
        .expect("the component compiles");
    assert_eq!(compiled.imports(&engine), ["outer#inner#f"]);
}

/// A component that imports what the world does not declare is refused at the
/// link.
///
/// The other side of the test above: the host defines the world's functions
/// and nothing else, so `foo` — a function by itself, outside every interface
/// — has no definition to link to, and instantiation is refused before
/// anything runs. A link that stubbed every import as a trap would accept it.
/// The nested `outer#inner#f` of the test before it is refused the same way,
/// which is what that test's own comment says of it.
#[tokio::test]
async fn a_component_importing_what_the_world_does_not_declare_is_refused_at_the_link() {
    let engine = nc_kernel::engine::Engine::new().expect("the engine's configuration is valid");
    for (component, import) in [
        (an_instance_inside_an_instance(), "outer"),
        (one_bare_function(), "foo"),
    ] {
        let compiled = engine.compile(&component).expect("the probe compiles");
        let refused = nc_kernel::activate_for_tests(&engine, &compiled)
            .await
            .expect_err("the import is not the world's");
        assert!(
            refused.contains(import) && !refused.contains("noctua:kernel/guest"),
            "the refusal is the import nothing defines, `{import}`, before the export is \
             looked for: {refused}"
        );
    }
}

/// The smallest component that imports the world's `log.write` and nothing else.
fn one_call_to_the_log() -> Vec<u8> {
    encoded(
        a_core_module_importing("noctua:kernel/log@0.2.0", "write", 3),
        "package nc:probe;\n\nworld probe {\n  import noctua:kernel/log@0.2.0;\n}\n",
    )
}

/// The smallest component that imports one function by itself, `foo`, taking
/// nothing and returning nothing.
fn one_bare_function() -> Vec<u8> {
    encoded(
        a_core_module_importing("$root", "foo", 0),
        "package nc:probe;\n\nworld probe {\n  import foo: func();\n}\n",
    )
}

/// `module` carrying the world `probe` written in `source`, encoded as a
/// component. The world is parsed beside the kernel's, so it can import from
/// it.
fn encoded(mut module: Vec<u8>, source: &str) -> Vec<u8> {
    let mut resolve = wit_parser::Resolve::default();
    resolve
        .push_path(concat!(env!("CARGO_MANIFEST_DIR"), "/wit"))
        .expect("the world parses");
    let probe = resolve
        .push_str("probe.wit", source)
        .expect("the probe's world parses against the kernel's");
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

/// A core module, byte for byte, whose one import is `field` from `module`.
///
/// The function takes `parameters` `i32`s and returns nothing — for `write`,
/// three: its level, then the message's pointer and length, as the canonical
/// ABI flattens them — and the module exports the one page of memory a message
/// is read from, under the name the lowering looks for.
fn a_core_module_importing(module: &str, field: &str, parameters: u8) -> Vec<u8> {
    let section = |id: u8, content: &[u8]| [&[id][..], &sized(content)].concat();
    [
        b"\0asm\x01\0\0\0".to_vec(),
        // The type section: one function type, the `i32`s in and nothing out.
        section(
            1,
            &[
                &[1, 0x60, parameters][..],
                &vec![0x7f; usize::from(parameters)],
                &[0],
            ]
            .concat(),
        ),
        // The import section: that type, imported as `module`'s `field`.
        section(
            2,
            &[
                &[1][..],
                &sized(module.as_bytes()),
                &sized(field.as_bytes()),
                &[0, 0],
            ]
            .concat(),
        ),
        // The memory section: one memory of one page and no maximum.
        section(5, &[1, 0, 1]),
        // The export section: that memory, as `memory`.
        section(7, &[&[1][..], &sized(b"memory"), &[2, 0]].concat()),
    ]
    .concat()
}

/// A component, byte for byte, whose one import is an instance `outer` that
/// exports an instance `inner` that exports `f`, a function taking nothing and
/// returning nothing.
///
/// Written out rather than encoded from WIT, which cannot say it: the component
/// model's own binary, a type section holding the outer instance's type — which
/// declares the inner instance's type and exports `inner` of it — and an
/// import section importing `outer` of that type.
fn an_instance_inside_an_instance() -> Vec<u8> {
    let section = |id: u8, content: &[u8]| [&[id][..], &sized(content)].concat();
    let inner = [
        // An instance type of two declarations:
        &[0x42, 2][..],
        // a function type, no parameters and no result,
        &[0x01, 0x40, 0, 0x01, 0x00],
        // and `f`, a function of that type, exported.
        &[0x04, 0x00],
        &sized(b"f"),
        &[0x01, 0],
    ]
    .concat();
    let outer = [
        // An instance type of two declarations:
        &[0x42, 2][..],
        // the inner instance's type,
        &[0x01],
        &inner,
        // and `inner`, an instance of that type, exported.
        &[0x04, 0x00],
        &sized(b"inner"),
        &[0x05, 0],
    ]
    .concat();
    [
        // The header: the component model's version and layer.
        b"\0asm\x0d\0\x01\0".to_vec(),
        // The type section: the outer instance's type.
        section(7, &[&[1][..], &outer].concat()),
        // The import section: `outer`, an instance of that type.
        section(10, &[&[1, 0x00][..], &sized(b"outer"), &[0x05, 0]].concat()),
    ]
    .concat()
}

/// `bytes`, after their length as the one LEB128 byte every length here fits in.
fn sized(bytes: &[u8]) -> Vec<u8> {
    let length = u8::try_from(bytes.len())
        .ok()
        .filter(|length| *length < 0x80)
        .expect("every length in the probe fits in one LEB128 byte");
    [&[length][..], bytes].concat()
}

/// Bytes that are a component of another world are refused before anything runs.
///
/// The control for the test above: a link that accepted anything would pass it.
/// The smallest component there is — the eight bytes of a header with no
/// section — compiles, exports no `guest`, and is refused at the link.
#[tokio::test]
async fn a_component_of_another_world_is_refused_at_the_link() {
    let engine = nc_kernel::engine::Engine::new().expect("the engine's configuration is valid");
    let compiled = engine
        .compile(b"\0asm\x0d\0\x01\0")
        .expect("an empty component compiles");
    let refused = nc_kernel::activate_for_tests(&engine, &compiled)
        .await
        .expect_err("a component that exports no guest is not a plugin");
    assert!(refused.contains("guest"), "{refused}");
}

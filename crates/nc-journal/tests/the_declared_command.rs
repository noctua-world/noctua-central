//! `journal.verify` is declared in the catalog; this is what answers it.
//!
//! **A declared command with nothing behind it reads as done.** A command
//! whose `args_schema` names arguments its entry point does not take — `table`
//! and `dry_run` on `limits.prune`, say — leaves the command a dispatcher will
//! call and the function that would answer it disagreeing, with nothing in the
//! tree saying so. `nc-prune` holds `limits.prune` to its declaration, and this
//! is the same check for `journal.verify`, so its declaration and its entry
//! point cannot drift apart in either direction.
//!
//! The dispatcher is the kernel's (`§11.4`): `crates/nc-kernel/src/builtin.rs`
//! reads the arguments this declaration names and holds its own end to it,
//! as this holds `Journal::verify`'s.

/// Every argument `Journal::verify` accepts, by the name the command declares.
///
/// Written out rather than derived, because a function's parameter names are not
/// available at runtime — and the point is that the two lists are compared.
const ARGUMENTS: [&str; 1] = ["from"];

#[test]
fn the_declared_command_is_the_one_this_crate_implements() {
    let declared = nc_catalog::command("journal.verify").expect("the command is declared");
    let schema = declared.args_schema;
    for argument in ARGUMENTS {
        assert!(
            declared.arguments.contains(&argument),
            "`verify` takes `{argument}` and the declared args_schema does not: {schema}"
        );
    }

    // And the other direction, which is the one that rots: a property added to
    // the declaration with nothing behind it. Read from `arguments`, which the
    // compiler emits from the same parse it validates the schema with — a test
    // parsing the JSON by hand can get it wrong, and one that did would leave
    // adding `"until"` to the schema green.
    for declared_argument in declared.arguments {
        assert!(
            ARGUMENTS.contains(declared_argument),
            "the declared args_schema has `{declared_argument}` and `verify` does not take it"
        );
    }
    assert_eq!(declared.arguments.len(), ARGUMENTS.len());

    // **`Read` and not mutating**, which is what a dispatcher decides on before
    // it calls anything here, so both are asserted.
    assert_eq!(declared.action, "Read");
    assert!(!declared.mutating, "verifying the chain changes nothing");
    assert!(
        matches!(declared.reversibility, nc_catalog::Reversibility::Read),
        "{:?}",
        declared.reversibility
    );
}

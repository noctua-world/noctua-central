//! `journal.verify` is declared in the catalog; this is what answers it.
//!
//! **A declared command with nothing behind it reads as done.** `limits.prune`
//! was declared with `table` and `dry_run` in its `args_schema` and `nc-prune`'s
//! entry point took neither — the command a dispatcher will call and the
//! function that would answer it disagreed, and nothing in the tree said so.
//! This is the same check for the other declared command, so the pair cannot
//! drift apart in either direction.
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
    // compiler emits from the same parse it validates the schema with — this
    // test parsed the JSON by hand and got it wrong, so adding `"until"` to the
    // schema left it green.
    for declared_argument in declared.arguments {
        assert!(
            ARGUMENTS.contains(declared_argument),
            "the declared args_schema has `{declared_argument}` and `verify` does not take it"
        );
    }
    assert_eq!(declared.arguments.len(), ARGUMENTS.len());

    // **`Read` and not mutating**, which is what a dispatcher decides on before
    // it calls anything here — and what an earlier registry had wrong in both
    // directions at once.
    assert_eq!(declared.action, "Read");
    assert!(!declared.mutating, "verifying the chain changes nothing");
    assert!(
        matches!(declared.reversibility, nc_catalog::Reversibility::Read),
        "{:?}",
        declared.reversibility
    );
}

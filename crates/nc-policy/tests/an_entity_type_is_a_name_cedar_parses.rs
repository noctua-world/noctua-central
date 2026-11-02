//! The entity types a command's schema may name, held to Cedar's own parser.
//!
//! `nc-schema` refuses an `x-entity` that is not a Cedar type name, and the
//! rule it refuses by is written by hand — identifiers joined by `::`, none of
//! them a word Cedar reserves — because the catalog compiler reaches that
//! crate and must not reach Cedar. A rule written by hand is a second copy of
//! Cedar's grammar, and this is where the two are compared: this crate already
//! runs Cedar, and depends on `nc-schema` for its tests alone.
//!
//! **One direction is a defect and the other a choice.** A name the schema
//! admits and Cedar refuses is a command the kernel registers whose resource
//! can never be built. A name Cedar parses and the schema refuses fails closed
//! when the command is declared; each such name is written down below.
//!
//! Found by review: seven of the nine reserved words could be dropped from
//! `nc-schema`'s list with every suite green, and `then` dropped from it
//! compiled `"x-entity": "then"`, which Cedar refuses.

use std::str::FromStr;

use cedar_policy::EntityTypeName;
use nc_schema::Schema;

/// The words Cedar reserves, spelled here and not read out of `nc-schema`.
///
/// The list there is what is judged, so a word dropped from it has to be one
/// this file still asks about.
const RESERVED: [&str; 9] = [
    "true", "false", "if", "then", "else", "in", "is", "like", "has",
];

/// Names Cedar parses as a type: what an author writes, and the grammar's edges.
const ORDINARY: [&str; 20] = [
    "Content",
    "Realm",
    "User",
    "Noctua::Content",
    "A::B::C",
    "_x",
    "_",
    "__",
    "User2",
    "principal",
    "action",
    "resource",
    "context",
    "Bool",
    "Long",
    "String",
    "Set",
    "Record",
    "ipaddr",
    "decimal",
];

/// Names neither side may read as a type.
const MALFORMED: [&str; 11] = [
    "",
    "1x",
    "Cont-ent",
    "Content Type",
    "A::",
    "::A",
    "A ::B",
    "A:: B",
    "é",
    "__cedar",
    "__cedar::X",
];

/// Names Cedar parses and the schema refuses.
///
/// The schema fails closed on every segment that opens `__cedar`, and Cedar
/// refuses only a segment that is `__cedar` itself.
const STRICTER: [&str; 2] = ["__cedarX", "__cedarish"];

/// Whether `nc-schema` compiles a schema whose entity argument names `name`.
fn admitted(name: &str) -> bool {
    let schema = serde_json::json!({
        "type": "object", "additionalProperties": false,
        "properties": {"p": {"type": "string", "maxLength": 128, "x-entity": name}},
        "required": ["p"]
    });
    Schema::compile(&schema.to_string()).is_ok()
}

/// Whether Cedar parses `name` as an entity type.
fn parsed(name: &str) -> bool {
    EntityTypeName::from_str(name).is_ok()
}

/// Every reserved word, bare and as either segment of a namespaced name.
fn reserved_names() -> Vec<String> {
    RESERVED
        .iter()
        .flat_map(|word| {
            [
                (*word).to_owned(),
                format!("Ns::{word}"),
                format!("{word}::Ns"),
            ]
        })
        .collect()
}

#[test]
fn no_word_cedar_reserves_is_admitted_as_an_entity_type() {
    let names = reserved_names();
    assert_eq!(names.len(), 27, "every word, in each of its three places");
    for name in &names {
        // Asked of Cedar first, so the case is about a name Cedar refuses and
        // not about a list that has stopped describing it.
        assert!(
            !parsed(name),
            "Cedar parses `{name}`, so this file's list of reserved words is stale"
        );
        assert!(
            !admitted(name),
            "`{name}` is admitted as an `x-entity` and Cedar refuses it, so a command \
             naming it would register with a resource no request can carry"
        );
    }
}

#[test]
fn every_entity_type_the_schema_admits_is_one_cedar_parses() {
    let mut asked = 0_usize;
    let mut admitted_here = 0_usize;
    let candidates = reserved_names()
        .into_iter()
        .chain(ORDINARY.iter().map(|name| (*name).to_owned()))
        .chain(MALFORMED.iter().map(|name| (*name).to_owned()))
        .chain(STRICTER.iter().map(|name| (*name).to_owned()));
    for name in candidates {
        asked += 1;
        if admitted(&name) {
            admitted_here += 1;
            assert!(
                parsed(&name),
                "`{name}` is admitted as an `x-entity` and Cedar refuses it"
            );
        }
    }
    assert_eq!(
        asked,
        27 + ORDINARY.len() + MALFORMED.len() + STRICTER.len()
    );
    // Every ordinary name, and nothing else: an implication over a schema
    // that admitted nothing would hold as well.
    assert_eq!(
        admitted_here,
        ORDINARY.len(),
        "the schema admitted a number of names other than the ordinary ones"
    );
}

#[test]
fn where_the_schema_and_cedar_disagree_the_schema_refuses_and_is_written_down() {
    for name in ORDINARY {
        assert!(
            parsed(name),
            "Cedar refuses `{name}`, so it is not ordinary"
        );
        assert!(
            admitted(name),
            "the schema refuses `{name}`, which Cedar parses"
        );
    }
    for name in MALFORMED {
        assert!(
            !parsed(name),
            "Cedar parses `{name}`, so it is not malformed"
        );
        assert!(!admitted(name), "the schema admits `{name}`");
    }
    for name in STRICTER {
        assert!(
            parsed(name) && !admitted(name),
            "`{name}` is no longer a name only the schema refuses; move it"
        );
    }
}

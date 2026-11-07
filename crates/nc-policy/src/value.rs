//! TOML in the case files, Cedar JSON out.
//!
//! Cedar's entity and context JSON has two shapes an ordinary TOML value cannot
//! express: an extension call (`datetime`) and an entity reference. Both are
//! written **explicitly** here — `{ datetime = "..." }` and `{ entity = "..." }`
//! — rather than inferred from what a string looks like.
//!
//! Inference was the alternative and it is the wrong one: a rule like "a string
//! shaped like a date becomes a datetime" turns a policy test into something
//! that depends on a regular expression, and the day somebody writes a grant
//! reason that reads like a date is the day a security test starts passing for a
//! new reason.

use serde_json::{Map, Value};

/// Why a case's context could not be turned into Cedar's JSON.
///
/// Crate-private: it is how `cases` reports a bad value, and a caller outside
/// sees the `Problem` that carries it rather than this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotAValue {
    /// The key path that failed, e.g. `context.grant`.
    pub(crate) at: String,
    /// What was wrong with it.
    pub(crate) what: String,
}

/// Converts one TOML value.
///
/// # Errors
///
/// Returns the key path and the reason when a value has no Cedar equivalent.
pub(crate) fn to_cedar(at: &str, value: &toml::Value) -> Result<Value, NotAValue> {
    match value {
        toml::Value::String(text) => Ok(Value::String(text.clone())),
        toml::Value::Integer(number) => Ok(Value::from(*number)),
        toml::Value::Boolean(flag) => Ok(Value::Bool(*flag)),
        toml::Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| to_cedar(&format!("{at}[{index}]"), item))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        toml::Value::Table(table) => table_to_cedar(at, table),
        // Deliberately unsupported, and each for its own reason. A float cannot
        // appear in a Cedar value at all (`§16.4` bans them from canonical JSON
        // for the same reason: no two implementations round them alike), and a
        // bare TOML datetime would be the inference this module exists to avoid.
        toml::Value::Float(_) => Err(NotAValue {
            at: at.to_owned(),
            what: "a float, which Cedar has no type for".to_owned(),
        }),
        toml::Value::Datetime(_) => Err(NotAValue {
            at: at.to_owned(),
            what: "a bare TOML datetime. Cedar's is an extension value, so write \
                   it as `{ datetime = \"2026-08-26T00:00:00Z\" }`"
                .to_owned(),
        }),
    }
}

fn table_to_cedar(at: &str, table: &toml::value::Table) -> Result<Value, NotAValue> {
    if table.len() == 1 {
        if let Some(toml::Value::String(argument)) = table.get("datetime") {
            let mut call = Map::new();
            call.insert("fn".to_owned(), Value::String("datetime".to_owned()));
            call.insert("arg".to_owned(), Value::String(argument.clone()));
            let mut outer = Map::new();
            outer.insert("__extn".to_owned(), Value::Object(call));
            return Ok(Value::Object(outer));
        }
        if let Some(toml::Value::String(uid)) = table.get("entity") {
            let (kind, id) = split_uid(uid).ok_or_else(|| NotAValue {
                at: at.to_owned(),
                what: format!(
                    "`{uid}` is not an entity reference. Write the whole uid, as in \
                     `Grant::\"g.current\"`"
                ),
            })?;
            let mut inner = Map::new();
            inner.insert("type".to_owned(), Value::String(kind));
            inner.insert("id".to_owned(), Value::String(id));
            let mut outer = Map::new();
            outer.insert("__entity".to_owned(), Value::Object(inner));
            return Ok(Value::Object(outer));
        }
    }
    let mut out = Map::new();
    for (key, value) in table {
        out.insert(key.clone(), to_cedar(&format!("{at}.{key}"), value)?);
    }
    Ok(Value::Object(out))
}

/// `Namespace::Type::"id"` into its type and its id.
///
/// Splits at the **first** `::"`. A type may be namespaced — `A::B::C` — but a
/// `"` cannot appear in one, so the first `::` that is followed by a quote is
/// always the separator. Splitting at the last one instead would cut an id
/// containing `::` in the wrong place.
pub(crate) fn split_uid(uid: &str) -> Option<(String, String)> {
    let at = uid.find("::\"")?;
    let (kind, rest) = uid.split_at(at);
    let id = rest.strip_prefix("::\"")?.strip_suffix('"')?;
    if kind.is_empty() {
        return None;
    }
    Some((kind.to_owned(), id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{split_uid, to_cedar};

    #[test]
    fn a_datetime_becomes_an_extension_call() {
        let value: toml::Value =
            toml::from_str("datetime = \"2026-08-26T00:00:00Z\"\n").expect("a table");
        let json = to_cedar("context.now", &value).expect("converts");
        assert_eq!(json["__extn"]["fn"], "datetime");
        assert_eq!(json["__extn"]["arg"], "2026-08-26T00:00:00Z");
    }

    #[test]
    fn an_entity_becomes_a_reference_and_keeps_a_namespaced_type() {
        let value: toml::Value =
            toml::from_str("entity = 'Noctua::Grant::\"g.current\"'\n").expect("a table");
        let json = to_cedar("context.grant", &value).expect("converts");
        assert_eq!(json["__entity"]["type"], "Noctua::Grant");
        assert_eq!(json["__entity"]["id"], "g.current");
    }

    /// A table with a sibling key is a record, not an entity reference.
    ///
    /// **The clause that says so is held here.** Weakened
    /// from `table.len() == 1` to `!table.is_empty()`, it turns
    /// `{ entity = "Grant::\"g\"", note = "…" }` into a bare entity reference
    /// with the sibling silently dropped, and every other test in this crate
    /// stays green — measured. A context
    /// this pack decides against is a record, so dropping a key changes the
    /// request without changing the file it came from.
    #[test]
    fn a_table_with_a_sibling_key_is_a_record_and_keeps_both() {
        let value: toml::Value =
            toml::from_str("entity = 'Grant::\"g.current\"'\nnote = 'kept'\n").expect("a table");
        let json = to_cedar("context.grant", &value).expect("converts");
        assert!(
            json.get("__entity").is_none(),
            "a two-key table became an entity reference and its sibling was dropped: {json}"
        );
        assert_eq!(json["note"], "kept");
        assert_eq!(json["entity"], "Grant::\"g.current\"");
        // And the same for the extension call beside it.
        let value: toml::Value =
            toml::from_str("datetime = '2026-08-26T00:00:00Z'\nnote = 'kept'\n").expect("a table");
        let json = to_cedar("context.now", &value).expect("converts");
        assert!(json.get("__extn").is_none(), "{json}");
        assert_eq!(json["note"], "kept");
    }

    #[test]
    fn an_id_may_contain_the_separator_itself() {
        // The reason the split is at the *first* `::"` and not the last: a type
        // cannot hold a quote, so the first `::` followed by one is always the
        // separator — and an id that holds `::` stays whole.
        let (kind, id) = split_uid("Noctua::Grant::\"a::b\"").expect("splits");
        assert_eq!(kind, "Noctua::Grant");
        assert_eq!(id, "a::b");
    }

    #[test]
    fn something_that_is_not_a_uid_at_all_is_refused() {
        assert!(split_uid("Grant").is_none());
        assert!(
            split_uid("::\"orphan\"").is_none(),
            "an empty type is not a type"
        );
        assert!(split_uid("Grant::\"unterminated").is_none());
    }

    #[test]
    fn an_ordinary_table_stays_an_ordinary_object() {
        // The other half: without this, the two shapes above would be indistinguishable
        // from any single-key table and every nested value would be mangled.
        let value: toml::Value = toml::from_str("via_agent = false\ndepth = 2\n").expect("a table");
        let json = to_cedar("context", &value).expect("converts");
        assert_eq!(json["via_agent"], false);
        assert_eq!(json["depth"], 2);
    }

    #[test]
    fn a_float_is_refused_and_the_path_says_where() {
        let value = toml::Value::Float(1.5);
        let error = to_cedar("context.budget", &value).expect_err("refused");
        assert_eq!(error.at, "context.budget");
    }

    #[test]
    fn a_bare_toml_datetime_is_refused_with_the_form_to_use() {
        let value: toml::Value = toml::from_str("now = 2026-08-26T00:00:00Z\n").expect("a table");
        let toml::Value::Table(table) = value else {
            unreachable!()
        };
        let error = to_cedar("context.now", &table["now"]).expect_err("refused");
        assert!(error.what.contains("{ datetime ="), "{}", error.what);
    }
}

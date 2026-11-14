//! The generated crate answers, and its lookups find what they claim to.
//!
//! `catalog/notifications.toml` says "`nc-catalog`'s tests use it to prove the
//! handle resolves in both locales", and these are those tests: they
//! call `message`, `ErrorCode::from_code`, `command` and `Locale::from_tag`.
//!
//! That matters most for `message`, which is a binary search over a table whose
//! sortedness is an **unasserted consequence** of the compiler happening to hold
//! its messages in a `BTreeMap` — while the workspace enables `toml`'s
//! `preserve_order` feature elsewhere on purpose. One type change and every
//! lookup silently returns the wrong sentence, or none, with no error anywhere.

use nc_catalog::{
    COMMANDS, EVENTS, Email, ErrorCode, Locale, Notification, RATE_LIMITS, STORES, TABLES, command,
    message,
};

/// A level orders the way a consumer's filter needs it to.
///
/// **`Level` orders** — `level >= Level::Warn` compiles — as `Severity` does,
/// whose own doc calls its order the scale. A minimum-level filter is the first
/// thing anybody reading a log writes.
#[test]
fn a_level_orders_from_least_noisy_to_most() {
    use nc_catalog::Level;
    assert!(Level::Error > Level::Warn);
    assert!(Level::Warn > Level::Info);
    assert!(Level::Info > Level::Debug);
    assert!(Level::Debug > Level::Trace);
    // The filter itself, which is the point: it keeps what is at least as loud
    // and drops the rest.
    let kept: Vec<Level> = [Level::Trace, Level::Info, Level::Warn, Level::Error]
        .into_iter()
        .filter(|level| *level >= Level::Warn)
        .collect();
    assert_eq!(kept, vec![Level::Warn, Level::Error]);
}

/// A BCP 47 tag names its locale whatever case it is written in.
///
/// **BCP 47 defines tags case-insensitively**, and an exact comparison gives:
///
/// ```text
/// pt-BR -> Some(PtBR)     pt-br -> None     PT-BR -> None
///    en -> Some(En)          EN -> None        En -> None
/// ```
///
/// A gateway reading `Accept-Language: pt-br` — the spelling browsers actually
/// send — would get `None` and serve the source locale with nothing said. The
/// prefix question (`en-GB` naming `en`) is a different one and is deliberately
/// still exact; this is only about case.
#[test]
fn a_tag_names_its_locale_in_any_case() {
    for locale in nc_catalog::Locale::ALL {
        let tag = locale.tag();
        for spelling in [tag.to_lowercase(), tag.to_uppercase(), tag.to_owned()] {
            assert_eq!(
                nc_catalog::Locale::from_tag(&spelling),
                Some(*locale),
                "`{spelling}` names `{tag}` and BCP 47 says so whatever case it is in"
            );
        }
    }
    // And a tag this platform does not ship is still nothing, in any case.
    assert_eq!(nc_catalog::Locale::from_tag("XX-yy"), None);
}

/// The crate root offers everything a consumer needs to show a sentence.
///
/// **`render` is re-exported at the root**, because it is the one that
/// stops `{field}` reaching a reader. Without it, a consumer
/// writing `use nc_catalog::*;` gets `message` — which returns the sentence
/// with its placeholders still in it — and `render` does not resolve:
///
/// ```text
/// error[E0425]: cannot find function `render` in this scope
///
/// message(en)      = "{field} must be between {min} and {max}."
/// i18n::render(en) = Ok("flake must be between 0 and 1023.")
/// ```
///
/// Written through the root path on purpose: `i18n::render` works either way,
/// and what this holds is the one line in the emitter that says so.
#[test]
fn the_root_offers_the_renderer_and_not_only_the_raw_sentence() {
    let key = "error.validation.out-of-range";
    let raw = nc_catalog::message(key, nc_catalog::Locale::En).expect("the key is declared");
    assert!(
        raw.contains('{'),
        "this key was chosen because its sentence carries placeholders: {raw}"
    );
    let shown = nc_catalog::render(
        key,
        nc_catalog::Locale::En,
        &[("field", "flake"), ("min", "0"), ("max", "1023")],
    )
    .expect("the key is declared")
    .expect("every placeholder was supplied");
    assert!(
        !shown.contains('{') && shown.contains("flake"),
        "the rendered sentence still carries a placeholder: {shown}"
    );
}

#[test]
fn every_key_resolves_which_is_what_proves_the_table_is_searchable() {
    // `message` bisects a table this test cannot see, and asserting that `KEYS`
    // is sorted instead checks a different array, which stays green with the
    // searched one reversed. What proves the search works is the search: an
    // unsorted table fails to find most of its own keys.
    //
    // The completeness guarantee is the other half, and it is the same loop:
    // every key resolves in every locale, or the build should not have passed.
    for key in nc_catalog::i18n::KEYS {
        for locale in Locale::ALL {
            let sentence = message(key, *locale)
                .unwrap_or_else(|| panic!("`{key}` resolves to nothing in {}", locale.tag()));
            assert!(!sentence.is_empty(), "`{key}` is empty in {}", locale.tag());
        }
    }
    assert!(
        nc_catalog::i18n::KEYS.len() > 10,
        "the table is suspiciously small, so this proves very little"
    );
}

#[test]
fn a_key_nothing_declares_resolves_to_nothing() {
    // The other half. Without it, a `message` that returned the first row for
    // everything would pass the test above.
    assert_eq!(message("error.there.is.no.such.key", Locale::En), None);
}

#[test]
fn the_two_locales_say_different_things() {
    // A generated table whose columns were both filled from the source locale
    // would satisfy every test above and ship an untranslated platform.
    let differing = nc_catalog::i18n::KEYS
        .iter()
        .filter(|key| message(key, Locale::En) != message(key, Locale::PtBR))
        .count();
    assert!(
        differing > nc_catalog::i18n::KEYS.len() / 2,
        "only {differing} of {} keys differ between locales",
        nc_catalog::i18n::KEYS.len()
    );
}

#[test]
fn a_locale_is_found_by_its_tag_and_an_unknown_tag_is_not() {
    for locale in Locale::ALL {
        assert_eq!(Locale::from_tag(locale.tag()), Some(*locale));
        assert_ne!(locale.name(), "");
        assert!(locale.dir() == "ltr" || locale.dir() == "rtl");
    }
    assert_eq!(Locale::from_tag("xx"), None);
    // Exact match only, and stated rather than discovered: `§10.9`'s locale
    // resolution order has an `Accept-Language` step, and whatever builds it
    // will need its own fallback from `en-GB` to `en`. This is where a reader
    // finds that out.
    assert_eq!(Locale::from_tag("en-GB"), None);
}

#[test]
fn every_error_code_round_trips_through_its_text() {
    // The boundary crossing: a code arrives from another service as text and
    // becomes a type again.
    assert_ne!(ErrorCode::ALL, []);
    for code in ErrorCode::ALL {
        assert_eq!(ErrorCode::from_code(code.code()), Some(*code));
        assert_eq!(code.to_string(), code.code());
        assert!(code.code().starts_with("NC-"));
        assert_ne!(code.description(), "");
        assert!(
            message(code.message_key(), Locale::En).is_some(),
            "{} points at a sentence that is not there",
            code.code()
        );
        // **A prefix, not a substring.** With `contains`,
        // `"NC-DB-0001".contains("")` is true — so an emitter writing `""` for
        // every area would pass. Both halves are asserted for every code.
        assert!(!code.area().is_empty(), "{} has no area", code.code());
        assert!(
            code.code().starts_with(&format!("NC-{}-", code.area())),
            "{} does not begin with its own area `{}`",
            code.code(),
            code.area()
        );
    }
    assert_eq!(ErrorCode::from_code("NC-NOPE-0001"), None);
}

#[test]
fn every_notification_and_email_resolves_both_of_its_keys() {
    // What `notifications.toml` says is tested.
    assert_ne!(Notification::ALL, []);
    for entry in Notification::ALL {
        for key in [entry.title_key(), entry.body_key()] {
            assert!(
                message(key, Locale::PtBR).is_some(),
                "`{key}` is not translated"
            );
        }
    }
    assert_ne!(Email::ALL, []);
    for entry in Email::ALL {
        for key in [entry.subject_key(), entry.body_key()] {
            assert!(
                message(key, Locale::PtBR).is_some(),
                "`{key}` is not translated"
            );
        }
    }
}

#[test]
fn a_command_is_found_by_name_and_carries_what_a_dispatcher_needs() {
    assert_ne!(COMMANDS, []);
    for entry in COMMANDS {
        assert_eq!(
            command(entry.name).map(|found| found.name),
            Some(entry.name)
        );
        assert!(!entry.action.is_empty(), "{} has no action", entry.name);
        assert!(
            !entry.resource_selector.is_empty(),
            "{} has no resource selector",
            entry.name
        );
        assert!(
            entry.args_schema.contains("\"object\""),
            "{}'s schema is not an object schema",
            entry.name
        );
        // **Its two sentences resolve, in both locales.** With errors,
        // notifications and emails checked and commands not, filtering every
        // `command.*` key out of the message table would leave the whole suite
        // green, with every command losing the name and the description a
        // palette shows.
        for (label, key) in [
            ("title", entry.title_key),
            ("description", entry.description_key),
        ] {
            for locale in Locale::ALL {
                assert!(
                    message(key, *locale).is_some(),
                    "{}'s {label} does not resolve in {}",
                    entry.name,
                    locale.tag()
                );
            }
        }
    }
    assert!(command("nothing.at.all").is_none());
}

#[test]
fn every_limit_the_pruner_reads_is_one_it_can_act_on() {
    assert_ne!(TABLES, []);
    assert_ne!(STORES, []);
    let prunable = TABLES
        .iter()
        .filter(|table| table.strategy == nc_catalog::Strategy::OldestFirst)
        .count();
    assert!(prunable > 0, "the pruner has no table to prune");
    for table in TABLES {
        assert!(
            !table.order_by.is_empty(),
            "{} has no ordering column",
            table.name
        );
        match table.strategy {
            nc_catalog::Strategy::OldestFirst => {
                let batch = table.prune_batch.expect("oldest-first carries a batch");
                let max = table.max_rows.expect("oldest-first carries a limit");
                assert!(
                    batch > 0 && batch <= max,
                    "{}'s batch is not usable",
                    table.name
                );
            }
            nc_catalog::Strategy::PolicyPack => {
                assert!(table.max_rows.is_none(), "{} has both bounds", table.name);
            }
            // User state: the writer refuses at the limit and nothing prunes,
            // so there is a limit and no batch.
            nc_catalog::Strategy::RefuseAtLimit => {
                assert!(
                    table.max_rows.is_some_and(|limit| limit > 0),
                    "{} refuses at no limit",
                    table.name
                );
                assert!(table.prune_batch.is_none(), "{} names a batch", table.name);
            }
        }
    }
    // **Both databases hold a bounded table**, so the gate that reads each
    // directory of migrations has an entry of its own database to hold a table to.
    for database in [
        nc_catalog::Database::Server,
        nc_catalog::Database::LocalFile,
    ] {
        assert!(
            TABLES.iter().any(|table| table.database == database),
            "no table of {database:?} is bounded"
        );
    }
}

#[test]
fn both_key_derivations_are_still_key_derivations() {
    // Two constants, not one: a device PIN is four to eight digits and a
    // password is at least twelve characters, and one shared cost either makes
    // login slow or makes the PIN cheap to attack offline.
    // Through a slice rather than as bare comparisons: the constants are known
    // at compile time, so a direct `assert!` folds to a constant and clippy
    // rightly says the assertion has no value at run time.
    let both = [
        ("password", nc_catalog::ARGON2ID_PASSWORD),
        ("device_pin", nc_catalog::ARGON2ID_DEVICE_PIN),
    ];
    for (which, argon) in both {
        assert!(
            argon.memory_kib >= 19_456,
            "{which} derives with too little memory"
        );
        assert!(argon.iterations >= 1 && argon.parallelism >= 1, "{which}");
        assert!(
            argon.salt_length >= 16 && argon.output_length >= 16,
            "{which}"
        );
    }
    assert!(
        both[1].1.memory_kib > both[0].1.memory_kib,
        "the PIN's derivation is not harder than the password's, and the PIN is the \
         smaller secret"
    );
}

#[test]
fn every_authentication_surface_carries_both_budgets_and_both_escalate() {
    assert_ne!(RATE_LIMITS, []);
    for limit in RATE_LIMITS {
        for (which, budget) in [
            ("per_identifier", limit.per_identifier),
            ("per_address", limit.per_address),
        ] {
            assert!(
                !budget.delay_ms.is_empty(),
                "{}'s {which} has no delay curve",
                limit.surface
            );
            assert!(
                budget.delay_ms.windows(2).all(|pair| pair[1] >= pair[0]),
                "{}'s {which} curve is not escalating",
                limit.surface
            );
            // **And it rises, which "does not fall" is not.** This test is
            // named for escalation, and asserting only that the curve never
            // drops lets a flat one satisfy it — measured with
            // `email_confirmation`'s per-identifier curve replaced by
            // `[1_000, 1_000, 1_000, 1_000, 1_000]`:
            // `ok 15 generated file(s), 2 rewritten`, `check passed` with no
            // warn line, and `docs/limits.md` republishing the flat curve under
            // its own sentence saying the curve escalates. `login` carries no
            // lockout by decision, so a flat one second there is 86 400
            // attempts a day for ever.
            assert!(
                budget.delay_ms.last() > budget.delay_ms.first(),
                "{}'s {which} curve ends where it began, and the last value repeats for \
                 ever — §15.5 asks for an escalating delay",
                limit.surface
            );
            assert!(budget.max_attempts > 0);
        }
    }
    assert!(
        nc_catalog::rate_limit("device_pin")
            .expect("declared")
            .per_identifier
            .on_lockout
            == Some(nc_catalog::OnLockout::WipeCredential),
        "the device PIN does not wipe its credential, which NOCTUA.md §10.6 requires"
    );
}

#[test]
fn every_event_the_registry_declares_reaches_the_generated_descriptors() {
    // A test that matches every `FieldKind` variant cannot fail on any data —
    // not even every field rewritten to the one kind the module names as a
    // hole. What is falsifiable is coverage: an emitter that dropped an
    // event, or a registry entry that never arrived, shows up as a name in one
    // list and not the other.
    let registry = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../catalog/log-events.toml"
    ))
    .expect("the registry is beside the crate it generates");
    let declared: Vec<String> = registry
        .lines()
        .filter_map(|line| line.trim().strip_prefix("name = \""))
        .filter_map(|rest| rest.split('"').next())
        .map(ToOwned::to_owned)
        .collect();

    assert!(
        !declared.is_empty(),
        "the registry declared nothing, so this proves nothing"
    );
    assert_eq!(
        declared.len(),
        EVENTS.len(),
        "the registry declares {} event(s) and {} reached the descriptors",
        declared.len(),
        EVENTS.len()
    );
    for name in &declared {
        let event = EVENTS
            .iter()
            .find(|event| event.name == name)
            .unwrap_or_else(|| panic!("`{name}` is declared and did not reach the descriptors"));
        assert!(!event.description.is_empty(), "{name} has no description");
        for field in event.fields {
            assert!(!field.name.is_empty(), "{name} has a field with no name");
        }
    }
}

#[test]
fn the_registry_uses_more_than_one_kind_of_field() {
    // Matching every `FieldKind` variant to a string literal and asserting the
    // set does not contain `"text"` cannot fail: no arm can produce it. `text`
    // is not a kind the registry has at all: the compiler refuses one, with a
    // line, when the catalog is read.
    //
    // What is falsifiable, and what the type wall actually needs, is that the
    // registry exercises more than one kind. Flattening every field to `U64`
    // satisfies any check that only asks whether the kinds exist, and it is
    // exactly what an emitter bug looks like. The values themselves are compared
    // against the registry in `xtask/tests/the_registry_reaches_the_code.rs`,
    // where both sides can be held at once.
    let kinds: std::collections::BTreeSet<String> = EVENTS
        .iter()
        .flat_map(|event| event.fields.iter())
        .map(|field| format!("{:?}", field.kind))
        .collect();
    assert!(kinds.len() >= 3, "the registry uses only {kinds:?}");
    // **An identifier kind, not `Id` specifically.** `Record::work_unit` is a
    // `Snowflake`, so a `work_unit` field of kind `Id` would mean converting
    // one through `OpaqueId::new`, the single constructor in this crate that
    // accepts arbitrary text. With `work_unit` a `snowflake`, no `Id` field is
    // left at all — the honest state rather than a defect. What the wall needs
    // exercised is the *identifier path*, and a Snowflake is on it.
    assert!(
        kinds.contains("Id") || kinds.contains("Snowflake"),
        "no field carries an identifier, and an identifier is what the log holds \
         instead of a name: {kinds:?}"
    );
}

#[test]
fn the_source_locale_is_first() {
    // **The rule the emitter's own doc comment states, checked.**
    // `catalog/i18n/` is read with `read_dir`, and one `sort_by` in
    // `locale_files` is what makes the order of `Locale::ALL` independent of the
    // filesystem — tmpfs returns creation order, ZFS returns hash order. Every
    // locale assertion in the suite looks a locale up by tag, which is
    // order-blind, so without this, deleting that sort would leave the whole
    // gate green with:
    //
    // ```text
    // /// Every locale, source locale first.
    // pub const ALL: &'static [Self] = &[Self::PtBR, Self::En];
    // ```
    //
    // `Locale::index()` is a position in this list, and it is what every column
    // of `MESSAGES` is keyed on — so the order moving is every message moving.
    assert_eq!(
        nc_catalog::Locale::ALL
            .first()
            .expect("there is at least one locale")
            .tag(),
        "en",
        "the source locale is not first: {:?}",
        nc_catalog::Locale::ALL
            .iter()
            .map(|locale| locale.tag())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        nc_catalog::Locale::ALL
            .first()
            .expect("there is at least one locale")
            .index(),
        0,
        "the source locale is first and does not index as first"
    );
}

/// Every sentence in every locale renders with the params its registry declares.
///
/// **So the declaration has a
/// consumer.** `ErrorCode::params`, `Notification::params` and `Email::params`
/// publish the placeholders a sentence interpolates, and `messages.rs` refuses
/// a declaration that disagrees with either locale — three mechanisms agreeing
/// about a set that, without this, nothing turns into a sentence. What a person
/// hitting `NC-VAL-0001` would be shown
/// is `{field} must be between {min} and {max}.`, braces included.
///
/// This is the loop that makes the agreement mean something: it renders every key
/// through the declaration alone, in both locales, and refuses a leftover brace.
#[test]
fn every_declared_key_renders_with_nothing_left_in_braces() {
    let mut rendered = 0_usize;
    let mut interpolated = 0_usize;
    // **An entry, not a key.** One declaration governs every sentence the entry
    // has, and the compiler's own rule is that a declared param is used
    // *somewhere* among them — a notification whose body interpolates `{name}`
    // and whose title does not is ordinary. Asserting it per sentence
    // contradicts the compiler, which a run of this test shows.
    let mut entries: Vec<(Vec<&str>, &[&str])> = ErrorCode::ALL
        .iter()
        .map(|code| (vec![code.message_key()], code.params()))
        .collect();
    for one in Notification::ALL {
        entries.push((vec![one.title_key(), one.body_key()], one.params()));
    }
    for one in Email::ALL {
        entries.push((vec![one.subject_key(), one.body_key()], one.params()));
    }
    // **`COMMANDS` included, and that is the whole difference.** A command's
    // title and description are catalog keys too, and without them no test
    // passes one through `render`. Measured, the same prose-brace sentence
    // in two places: on `error.canon.unparsable` this test catches it, and on
    // `command.limits.prune.description`, left out, `cargo xtask check` prints
    // `check passed`. A gate whose subject is "every declared key" has to mean
    // every one.
    for one in COMMANDS {
        entries.push((vec![one.title_key, one.description_key], &[]));
    }
    for (keys, params) in &entries {
        // A value that is not any placeholder's name, so a sentence that ignored
        // its declaration and interpolated something else still fails below.
        let values: Vec<(&str, &str)> = params.iter().map(|name| (*name, "…")).collect();
        for locale in Locale::ALL {
            let mut used_anywhere = false;
            for key in keys {
                let sentence = nc_catalog::i18n::render(key, *locale, &values)
                    .unwrap_or_else(|| panic!("`{key}` is declared and does not resolve"))
                    // **Named, because a bare `{error}` prints the placeholder
                    // and nothing else.** `"type": "object"` on its own does not
                    // say which key, which locale, or what went wrong.
                    .unwrap_or_else(|unfilled| {
                        panic!(
                            "`{key}` in {} interpolates `{{{unfilled}}}` and this test \
                             supplied only what its registry declares",
                            locale.tag()
                        )
                    });
                assert!(
                    !sentence.contains('{'),
                    "`{key}` rendered as `{sentence}` in {} with what it declares",
                    locale.tag()
                );
                rendered += 1;
                used_anywhere |= sentence.contains('…');
            }
            if !params.is_empty() {
                interpolated += 1;
                assert!(
                    used_anywhere,
                    "{keys:?} declare {params:?} and no {} sentence among them used one",
                    locale.tag()
                );
            }
        }
    }
    // Two floors. The first is the walk: an empty `declared` would satisfy every
    // assertion above by never entering the loop. The second is the point — a
    // tree where no sentence interpolates anything would pass the brace check
    // with the renderer deleted.
    assert!(rendered > 60, "only {rendered} sentence(s) were rendered");
    assert!(
        interpolated > 0,
        "no declared key interpolates anything, so this proves only that plain \
         sentences come back unchanged"
    );
}

/// A rendered sentence is the one the locale asked for.
///
/// **This is what holds `render` to its `locale` argument.** Every other
/// assertion about it is locale-agnostic — a brace check, a substitution check
/// — so without this the whole parameter can be discarded:
///
/// ```rust,ignore
/// pub fn render(key: &str, locale: Locale, values: &[(&str, &str)])
///     -> Option<Result<String, &'static str>> {
///     let _ = locale;
///     Some(fill(message(key, Locale::En)?, values))
/// }
/// ```
///
/// `cargo xtask check` → `check passed`. Every rendered sentence on the
/// platform in English, for every reader, with the gate green:
///
/// ```text
/// render(pt-BR) = flake must be between 0 and 1023.
/// message(pt-BR)= {field} precisa estar entre {min} e {max}.
/// ```
///
/// So this holds the rendered string against `message` in the *same* locale,
/// key by key, rather than against the other locale's — a comparison that says
/// they differ would pass on any two wrong sentences that happen not to match.
#[test]
fn a_rendered_sentence_is_the_one_its_own_locale_holds() {
    let mut checked = 0_usize;
    let mut differ = 0_usize;
    for code in ErrorCode::ALL {
        let key = code.message_key();
        let values: Vec<(&str, &str)> = code.params().iter().map(|name| (*name, "…")).collect();
        let mut seen: Vec<String> = Vec::new();
        for locale in Locale::ALL {
            let source = message(key, *locale).expect("every declared key resolves");
            let rendered = nc_catalog::i18n::render(key, *locale, &values)
                .expect("the key resolves")
                .expect("its declaration supplies every placeholder");
            // The rendered sentence is the locale's own, with the values put in
            // and nothing else changed: the parts either side of a placeholder
            // survive verbatim, so a renderer reading another locale's row fails
            // on the first key the two word differently.
            for fragment in source
                .split(['{', '}'])
                .filter(|part| part.len() > 3 && !code.params().contains(part))
            {
                assert!(
                    rendered.contains(fragment),
                    "`{key}` rendered in {} as `{rendered}`, which does not hold `{fragment}` \
                     from that locale's own sentence `{source}`",
                    locale.tag()
                );
            }
            seen.push(rendered);
            checked += 1;
        }
        if seen.windows(2).any(|pair| pair[0] != pair[1]) {
            differ += 1;
        }
    }
    // Two floors. The walk, and the thing that makes the walk able to fail: if
    // every locale worded every sentence identically, the fragment check above
    // would pass with `locale` discarded.
    assert!(checked > 50, "only {checked} sentence(s) were rendered");
    assert!(
        differ > 20,
        "only {differ} code(s) are worded differently across the locales, so this test \
         cannot tell a renderer that ignores its locale from one that does not"
    );
}

/// A placeholder with no value is refused, and the refusal names it.
///
/// The failure this exists to stop is silent: without it `render` would write
/// `{min}` into a sentence and return `Ok`. The withheld param is the *last*
/// one, because stopping at the first placeholder would pass a test that
/// withheld the first.
#[test]
fn a_placeholder_with_no_value_is_refused_rather_than_written() {
    let code = ErrorCode::ValidationOutOfRange;
    let params = code.params();
    assert!(
        params.len() > 1,
        "this test withholds the last of several and needs several"
    );
    let (last, rest) = params.split_last().expect("checked non-empty above");
    let values: Vec<(&str, &str)> = rest.iter().map(|name| (*name, "…")).collect();
    let unfilled = nc_catalog::i18n::render(code.message_key(), Locale::En, &values)
        .expect("the key resolves")
        .expect_err("a placeholder with no value is refused");
    assert_eq!(unfilled, *last);
    // And the sentence really did hold it, so the name came from the message
    // table rather than from the argument being echoed back.
    let sentence = message(code.message_key(), Locale::En).expect("the key resolves");
    assert!(sentence.contains(&format!("{{{unfilled}}}")), "{sentence}");
}

/// A key nothing declares renders to nothing, exactly as `message` resolves it.
#[test]
fn rendering_an_unknown_key_answers_nothing_rather_than_an_empty_sentence() {
    assert!(nc_catalog::i18n::render("error.nothing.here", Locale::En, &[]).is_none());
}

/// A registry entry cannot be built from outside the crate.
///
/// **The types are `Copy` and their fields `pub`**, so without the seal a
/// consumer could take a real command and hand a dispatcher something else:
///
/// ```text
/// catalog  : action=Delete mutating=true  rev=Write      -> checked against action
/// forged   : action=Read   mutating=false rev=Read       -> read-only, waved through
/// invented : name=realm.delete_everything                -> read-only, waved through
/// ```
///
/// A compile-fail fixture would be the direct proof and this crate has no
/// harness for one, so the assertion is on the attribute in the generated
/// source: `#[non_exhaustive]` is what makes the struct literal above stop
/// compiling outside `nc-catalog`.
///
/// **The literal, and not the `forged` row.** The fields are `pub` on a `Copy`
/// type, so a copy of a real entry can be edited into that row from any crate —
/// measured for `KERNEL` and for a `CapabilitySpec` from `nc-kernel` — and what
/// keeps an edited copy from deciding anything is a consumer that reads the
/// registry itself rather than taking an entry from its caller, which the
/// kernel's own sources are read for.
#[test]
fn a_registry_entry_is_sealed_against_construction() {
    // The fourteen, at the top of the body: clippy refuses an item after a
    // statement, and this is the list the whole test is written around.
    const SEALED: [&str; 14] = [
        // The registry entry a dispatcher checks an action against.
        "Command",
        "EventSpec",
        "FieldSpec",
        // The three bounds `catalog/limits.toml` declares.
        "RateLimit",
        "Budget",
        "StoreLimit",
        // And the key-derivation parameters, which a hand list written before
        // them would miss.
        "Argon2Id",
        // The kernel's bounds on a plugin, and what a capability grants. Sealed
        // against a literal only: an edited copy of `KERNEL` or of a spec
        // compiles anywhere, and what keeps one from deciding anything is that
        // no function of the kernel takes either from its caller, which
        // `crates/nc-kernel/src/limits.rs` reads every kernel source for.
        "KernelLimits",
        "CapabilitySpec",
        "Ungated",
        // The daemon's bounds on itself, sealed as the kernel's are: the daemon
        // reads `DAEMON` and takes no value of the type from its caller.
        "DaemonLimits",
        // A wiki text, whose texts a surface shows as the entry's: one built
        // elsewhere would be a text the catalog never judged.
        "WikiText",
        // A set of categories and a category: one built elsewhere would be a
        // name an entry could be listed under that the catalog never held.
        "CategorySet",
        "Category",
    ];

    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR")));
    // **Read out of the emitter, not written down here.** A hand list of five
    // names against an emitter that writes seven — `StoreLimit` and `Argon2Id`
    // named by no assertion — leaves the seal on a store's bound and on the
    // key-derivation parameters as whatever `emit.rs` last happened to write. A
    // hand list agrees with the emitter about anything neither of them knows,
    // which is exactly what arrives after it is written.
    let emitter = std::fs::read_to_string(root.join("../nc-catalog-compiler/src/emit.rs"))
        .expect("the emitter is the crate beside this one");
    // Assembled, so this line is not itself one of the emissions it counts.
    let emits = format!("#[non_exhaustive]\\n{} ", "pub struct");
    let sealed: std::collections::BTreeSet<String> = emitter
        .match_indices(&emits)
        .filter_map(|(at, _)| {
            emitter[at + emits.len()..]
                .split_whitespace()
                .next()
                .map(str::to_owned)
        })
        .collect();

    // **The set, compared for equality, and the loop below walks the written
    // half of it.** A count cannot carry a set: `sealed.len() >= 7` over a
    // `Vec` derived from a raw-text scan of the emitter with no comment
    // stripping has two ways past it, both measured:
    //
    //   `Argon2Id` unsealed in `emit.rs` and in the committed output
    //     -> it leaves the *derived* set, so the loop simply stops checking it
    //   plus one line of ordinary emitter prose spelling the same two tokens
    //     -> the count is seven again
    //     -> 22 passed, and `cargo xtask check` byte-identical
    //
    // So the names are written here, the derivation is compared against them
    // both ways — an eighth struct sealed by nobody, or a seventh gone — and
    // the loop below reads *this* list rather than the derivation. That is what
    // makes the prose route pointless: whatever `emit.rs`'s text says, all
    // seven are then looked for in the generated sources that ship, where an
    // unsealed one fails on the attribute it is missing.
    let written: std::collections::BTreeSet<String> =
        SEALED.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(
        written.len(),
        SEALED.len(),
        "`SEALED` names the same struct twice, so it stands for fewer than {} of them",
        SEALED.len()
    );
    assert_eq!(
        sealed, written,
        "the emitter seals {sealed:?} and this repository's sealed registry types are \
         {written:?}. A struct whose fields a dispatcher decides on is added to both in \
         one edit; a struct that left this set was unsealed, and a name here the emitter \
         does not write is a scan reading prose rather than emissions"
    );

    for name in SEALED {
        let needle = format!("pub struct {name} {{");
        let mut holding: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(root.join("src"))
            .expect("the generated sources are committed")
            .flatten()
        {
            let path = entry.path();
            if path.extension().is_none_or(|kind| kind != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("generated and committed");
            let Some(at) = source.find(&needle) else {
                continue;
            };
            assert!(
                source[..at].ends_with("#[non_exhaustive]\n"),
                "`{name}` in {} can be built by anybody, and its fields are what a \
                 dispatcher decides on",
                path.display()
            );
            holding.push(entry.file_name().to_string_lossy().into_owned());
        }
        // **Exactly one file declares it**, which is also the floor under the
        // assertion above: a name found in no generated source would walk the
        // loop without ever reaching the attribute check.
        assert_eq!(
            holding.len(),
            1,
            "the emitter seals `{name}` and {} generated file(s) declare it: {holding:?}",
            holding.len()
        );
    }
    // **And no tally of the loop's own iterations.**
    // `assert_eq!(checked, sealed.len())` over a `checked += 1` that is the
    // unconditional last statement of the body is a count of how many times a
    // `for` loop went round, compared against the length of what it went round.
    // It cannot fail.
}

/// Every generated enum stays open, which is the catalog's whole promise.
///
/// **The exact complement of the test above.** `emit.rs` carries the paragraph
/// — *"Deliberately NOT `#[non_exhaustive]`… A new error code becoming a
/// compile error at every site that handles them is the catalog's whole promise
/// (§15.2), and that attribute is what would break it"* — and a paragraph is
/// not a check. Written into the emitter, the attribute is invisible to
/// `cargo xtask build`, which rewrites the generated files to match, and to
/// every `match` inside this crate; what it breaks is an exhaustive `match`
/// outside the crate. Such a `match` covers some generated enums by accident,
/// in this crate's own suite and in `xtask`'s pages test, and others not at
/// all, so without this test an enum no such `match` covers could be sealed
/// with `cargo xtask check` passing.
///
/// Asked in both places for the reason the struct test is: the emitter, so the
/// attribute cannot be introduced, and the sources that ship, so it cannot
/// arrive any other way.
#[test]
fn a_generated_enum_stays_open_so_a_new_variant_is_a_compile_error() {
    // Every generated enum, at the top of the body for the reason `SEALED` is
    // at the top of its own: clippy refuses an item after a statement.
    const OPEN: [&str; 13] = [
        // A host function names the one it asks for, so a capability added is a
        // compile error at every `match` over them rather than one arm short.
        "Capability",
        "Database",
        "Email",
        "ErrorCode",
        "FieldKind",
        "Level",
        "Locale",
        "Notification",
        "OnLockout",
        "Reversibility",
        "Severity",
        "Strategy",
        // A format a renderer matches on, so a format added is a compile error
        // at every renderer rather than a text shown as another format.
        "WikiFormat",
    ];

    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR")));
    let emitter = std::fs::read_to_string(root.join("../nc-catalog-compiler/src/emit.rs"))
        .expect("the emitter is the crate beside this one");
    // **Every attribute the emitter writes, not the one line before
    // `pub enum`.** Every generated type carries a `#[derive(…)]` on that line,
    // so asking for `#[non_exhaustive]` immediately above the declaration lets
    // the attribute written *above the derive* — the ordinary placement, and
    // what rustc itself accepts — seal an enum with this rule green. Measured:
    // `Notification` sealed that way in the emitter and in the shipped source
    // leaves such a rule passing and `check passed`, while a compile probe from
    // outside the crate answers `E0004: non-exhaustive patterns`.
    //
    // So the question is about the *count* rather than about the position: every
    // `#[non_exhaustive]` this emitter writes must be the one that seals a
    // struct, and any other is one sealing something else. Assembled, so these
    // lines are not themselves emissions this counts.
    //
    // **Every occurrence of it, not every one with a newline in the literal.**
    // Anchored on the attribute *followed by* `\n`, which is the shape
    // `out.push_str` writes, the count misses `writeln!`, the other spelling
    // this file uses, which puts the newline in the macro rather than in the
    // literal. Measured: one `writeln!(out, …)` of the attribute above the
    // notification enum ships `Notification` sealed with such a count green,
    // `check passed`, `cargo fmt` clean and the drift gate quiet, and an
    // exhaustive `match` from outside the crate then fails `E0004`. So the
    // attribute is counted however it is spelled, and the pair below is the
    // only thing that excuses one.
    //
    // Comments come out first: this file argues four times, in prose, about the
    // types that are deliberately *not* sealed, and a sentence is not an
    // emission. The strip is the crudest one there is — everything from `//` to
    // the end of the line — because nothing else on those lines is being read,
    // and no emission in this file has a `//` before it.
    let code = emitter
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(before, _)| before))
        .collect::<Vec<&str>>()
        .join("\n");
    let attribute = concat!("#[non_", "exhaustive]");
    let on_a_struct = format!("{attribute}{}{} ", "\\n", "pub struct");
    let written = code.matches(attribute).count();
    let sealing_a_struct = code.matches(&on_a_struct).count();
    assert_eq!(
        written, sealing_a_struct,
        "the emitter writes {written} `#[non_exhaustive]` and {sealing_a_struct} of them \
         seal a registry struct. The rest seal something else, and a sealed registry \
         enum is a new error code that compiles everywhere it is handled instead of \
         failing at every site"
    );

    // And the generated sources that ship, where an attribute could arrive
    // without the emitter ever writing one.
    let mut found: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(root.join("src"))
        .expect("the generated sources are committed")
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_none_or(|kind| kind != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("generated and committed");
        for (at, _) in source.match_indices("pub enum ") {
            let name = source[at + "pub enum ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
            // The attributes above it, innermost last — not the single line
            // before it, which is always the `#[derive(…)]` every generated
            // type carries. **And a comment is part of the block, not the end
            // of it.** A walk back that stops when the line is no longer an
            // attribute is stopped by the `///` every generated type carries —
            // and the attribute written *above* the doc comment, which is
            // ordinary placement and what rustc accepts, is never looked at.
            // That is the same shape this rule's own paragraph above records
            // one position earlier: past the `#[derive(…)]`, and stopped at the
            // doc.
            let sealed = source[..at]
                .lines()
                .rev()
                .take_while(|line| {
                    let line = line.trim_start();
                    line.starts_with("#[") || line.starts_with("//")
                })
                .any(|line| line.trim() == concat!("#[non_", "exhaustive]"));
            assert!(
                !sealed,
                "`{name}` in {} is sealed, so a variant added to it is not a compile \
                 error at the sites that handle it",
                path.display()
            );
            found.insert(name);
        }
    }
    // **The set, compared for equality**, which is the floor as well: a scan
    // that found nothing would satisfy every assertion above it.
    let written: std::collections::BTreeSet<String> =
        OPEN.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(
        found, written,
        "this repository's open registry enums are {written:?} and the generated \
         sources declare {found:?}. An enum added to the registry is added here in the \
         same edit, and one that left this set is one nothing now asks about"
    );
}

/// `as_str` answers the word the registry itself writes.
///
/// **Every enum here has a text form**, so no consumer that needs one writes
/// its own: without it, `nc-log` needs a `level_name`, the pages test has
/// a `severity_word`, and `{:?}` gives `Critical` where the ledger and both
/// pages write `critical`. `Severity`'s own doc names the hazard — "two
/// vocabularies for one judgement is how they drift apart" — and that would be
/// three.
///
/// Compared against the registry text on disk, not against a list written here:
/// the ledger records each code's severity word, and `catalog/log-events.toml`
/// records each event's level. `xtask`'s pages test keeps its own independent
/// list on purpose, because a comparison relates two things and one of them has
/// to be free of the emitter.
#[test]
fn every_word_an_enum_answers_is_the_word_the_registry_writes() {
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let ledger = std::fs::read_to_string(root.join("catalog/errors.reserved.toml"))
        .expect("the ledger is committed");
    let mut severities = 0_usize;
    for code in nc_catalog::ErrorCode::ALL {
        let line = ledger
            .lines()
            .find(|line| line.starts_with(&format!("\"{}\"", code.code())))
            .unwrap_or_else(|| panic!("{} is reserved", code.code()));
        // `"NC-…" = "<key> — <severity> — retryable:… — params:[…] — <description>"`
        let recorded = line
            .split(" — ")
            .nth(1)
            .expect("the meaning has a severity");
        assert_eq!(
            code.severity().as_str(),
            recorded,
            "`{}` answers a severity the ledger does not record",
            code.code()
        );
        severities += 1;
    }
    assert!(severities > 20, "only {severities} code(s) were read");

    let events = std::fs::read_to_string(root.join("catalog/log-events.toml"))
        .expect("the registry is committed");
    let mut levels = 0_usize;
    for event in nc_catalog::EVENTS {
        // **The entry, not everything after its name.** TOML fixes no order on
        // the keys of a table, so an entry writing `level` above `name` would
        // send a scan of everything after the name past its own block and into
        // the *next* `[[event]]`. Measured: `level` moved above `name` for
        // `journal.appended` — a reordering the generated output is
        // byte-identical under, both drift tests still passing — turns such a
        // `check` red with `` `journal.appended` answers a level the registry
        // does not declare `` and `right: "info"`, which is the neighbouring
        // event's level. A gate wrong in that direction sends a reader to look
        // for a defect that is not there.
        //
        // It is vacuous in the other direction too, on the same reading:
        // `log.dropped` and `log.store_unreachable` are both `warn`, so for
        // that pair a wrong generated level of `warn` would be compared against
        // the neighbour's `warn` and pass.
        let block = events
            .split("\n[[event]]")
            .find(|block| block.contains(&format!("name = \"{}\"", event.name)))
            .unwrap_or_else(|| panic!("{} is declared", event.name));
        let recorded = block
            .lines()
            .find_map(|line| line.trim().strip_prefix("level = "))
            .expect("every event declares a level")
            .trim_matches('"');
        assert_eq!(
            event.level.as_str(),
            recorded,
            "`{}` answers a level the registry does not declare",
            event.name
        );
        levels += 1;
    }
    assert!(levels > 5, "only {levels} event(s) were read");
}

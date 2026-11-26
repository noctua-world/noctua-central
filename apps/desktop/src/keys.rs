//! The keys the window is driven by (`NOCTUA.md §12.8`, §12.9): the Leader
//! tapped or held, the keys pressed with it held, and the keys of the leader
//! menu — each a physical key, each resolved to the command a binding in force
//! names, and nothing run that is not one.
//!
//! **A key is a physical key**: the page tells a key by `KeyboardEvent.code`,
//! which names where the key is and never what it types, and [`key_of`] reads
//! it as `nc_schema::keys` names the vocabulary — so neither Shift nor a
//! keyboard layout turns one key into another, and a code outside the
//! vocabulary is no key at all.
//!
//! **The bindings in force are the channel's** ([`Keymap`]): the catalog's
//! defaults with the person's own over them, read through
//! `nc_channel::keys::effective` when the window opens, so a binding is data —
//! a key and the name of the command it runs, never an argument and never a
//! closure — and the window runs nothing a binding does not name.
//!
//! **Tap or hold is decided here, from what the window hears** ([`Leader`]):
//! the system's registration reports the Leader going down and never up, so a
//! press heard while the window does not have the keyboard is a tap at once —
//! it shows the interface (`leader.tap`) — and one the window has the keyboard
//! for begins a hold, as a press the page itself saw does. While the Leader's
//! modifiers stay down, each other key is a key pressed with the Leader held,
//! run at once by the binding of its table — the chord modifier held as well
//! makes it the other table's — and the menu is never opened; once they are
//! let go, a hold in which no other key went down was a tap.
//!
//! **What each press runs** ([`Act`]): a tap is `leader.tap`, as the CLI's
//! `noctua leader` and the daemon's listener dispatch it, and every other
//! press the command its binding names, dispatched as the person — so a key
//! does nothing the command list could not (§12.8, §2.8).
//!
//! **Decided here, where `NOCTUA.md` is silent**: the Leader is held while its
//! modifiers are, whether or not its own key still is, since a hand lets the
//! key go first; a key pressed with the Leader's modifiers no longer all down
//! ends the hold, a tap where nothing else was pressed; and the window losing
//! the keyboard ends a hold with no tap.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! A page naming a key that is not one, or a binding that is not in force:
//! a key is one of the closed vocabulary or nothing, and a press runs only the
//! command its binding in force names. **What it does not answer**: the
//! layout's own label for a key, which this window does not read — a key is
//! labelled as a US board labels it.

use nc_schema::keys::{Binding, Key, Keyboard, LeaderPart, Modifier, Table, Tables};

/// The key `code` names, as `KeyboardEvent.code` writes it; nothing for a
/// code outside the vocabulary — a modifier, a lock, the numpad, Esc.
#[must_use]
pub fn key_of(code: &str) -> Option<Key> {
    Key::of_code(code)
}

/// The keyboard this build labels keys for: its system's.
#[must_use]
pub const fn keyboard() -> Keyboard {
    match nc_platform::clipboard::SYSTEM {
        nc_platform::clipboard::System::Linux => Keyboard::Linux,
        nc_platform::clipboard::System::Windows => Keyboard::Windows,
        nc_platform::clipboard::System::MacOs => Keyboard::Mac,
    }
}

/// The keys in force: the Leader, its chord modifier and every binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    /// The Leader and the chord modifier.
    part: LeaderPart,
    /// Every binding in force, beside the command it runs.
    tables: Tables,
}

/// One entry of the leader menu: its key and the command it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The key pressed in the menu.
    pub key: Key,
    /// The command it runs.
    pub command: String,
}

impl Keymap {
    /// The keys `effective` puts in force.
    #[must_use]
    pub fn of(effective: &nc_channel::keys::Effective) -> Self {
        Self {
            part: effective.part(),
            tables: effective.tables.clone(),
        }
    }

    /// The keys `part` and `tables` make.
    #[must_use]
    pub const fn new(part: LeaderPart, tables: Tables) -> Self {
        Self { part, tables }
    }

    /// The Leader and the chord modifier.
    #[must_use]
    pub const fn part(&self) -> LeaderPart {
        self.part
    }

    /// The leader menu's entries, key by key: every binding in force of the
    /// menu's table.
    #[must_use]
    pub fn menu(&self) -> Vec<Entry> {
        self.tables
            .iter()
            .filter(|(binding, _)| binding.table == Table::Menu)
            .map(|(binding, command)| Entry {
                key: binding.key,
                command: command.to_owned(),
            })
            .collect()
    }

    /// The command `key` runs in the leader menu, if one is bound to it.
    #[must_use]
    pub fn in_menu(&self, key: Key) -> Option<&str> {
        self.tables.command(Binding {
            table: Table::Menu,
            key,
        })
    }

    /// The command `key` runs pressed with the Leader held — and the chord
    /// modifier as well, where `with_chord` — if one is bound to it.
    #[must_use]
    pub fn held(&self, key: Key, with_chord: bool) -> Option<&str> {
        self.tables.command(Binding {
            table: if with_chord {
                Table::HeldWithChord
            } else {
                Table::Held
            },
            key,
        })
    }

    /// Whether `command` is one the leader menu runs.
    #[must_use]
    pub fn is_entry(&self, command: &str) -> bool {
        self.menu().iter().any(|entry| entry.command == command)
    }

    /// What the page is told of the keys, as the argument of
    /// `window.noctua.keys`: the Leader's key by its code, which of Control,
    /// Alt, Shift and Meta it holds, where the chord modifier is among those
    /// four, and the codes the menu binds.
    #[must_use]
    pub fn told(&self) -> nc_canon::Value {
        use nc_canon::Value;
        let chord = self.part.leader.chord();
        let held: Vec<Value> = Modifier::ALL
            .iter()
            .map(|modifier| Value::Bool(chord.modifiers().holds(*modifier)))
            .collect();
        let at = Modifier::ALL
            .iter()
            .position(|modifier| *modifier == self.part.chord)
            .unwrap_or(0);
        Value::object([
            ("code", Value::text(chord.key().code())),
            ("mods", Value::Array(held)),
            (
                "chord",
                Value::Integer(i64::try_from(at).unwrap_or_default()),
            ),
            (
                "menu",
                Value::Array(
                    self.menu()
                        .iter()
                        .map(|entry| Value::text(entry.key.code()))
                        .collect(),
                ),
            ),
        ])
    }
}

/// What a press comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// The Leader was tapped: `leader.tap`.
    Tap,
    /// A binding in force runs this command, with no argument.
    Run(String),
}

/// The Leader as the window hears it: idle, or held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Leader {
    /// While it is held: whether another key went down since.
    held: Option<bool>,
}

impl Leader {
    /// Whether it is held.
    #[must_use]
    pub const fn is_held(self) -> bool {
        self.held.is_some()
    }

    /// The system's registration heard it go down: a tap at once where the
    /// window does not have the keyboard, since nothing will tell it go up;
    /// a hold begun where it does, which the page is then told of.
    pub const fn pressed(&mut self, focused: bool) -> Option<Act> {
        if !focused {
            return Some(Act::Tap);
        }
        if self.held.is_none() {
            self.held = Some(false);
        }
        None
    }

    /// The page saw it go down.
    pub const fn down(&mut self) {
        if self.held.is_none() {
            self.held = Some(false);
        }
    }

    /// The page saw `key` go down with the Leader's modifiers held — and the
    /// chord modifier, where `with_chord`: the command its binding in force
    /// names, run at once.
    pub fn key(&mut self, key: Key, with_chord: bool, keymap: &Keymap) -> Option<Act> {
        self.held?;
        self.held = Some(true);
        keymap
            .held(key, with_chord)
            .map(|command| Act::Run(command.to_owned()))
    }

    /// The page saw its modifiers let go: a tap where no other key went down
    /// while it was held.
    pub const fn up(&mut self) -> Option<Act> {
        match self.held.take() {
            Some(false) => Some(Act::Tap),
            Some(true) | None => None,
        }
    }

    /// The window lost the keyboard: a hold ends, with no tap.
    pub const fn blurred(&mut self) {
        self.held = None;
    }
}

/// What a key pressed in the leader menu runs: the command bound to it there.
#[must_use]
pub fn in_menu(key: Key, keymap: &Keymap) -> Option<Act> {
    keymap
        .in_menu(key)
        .map(|command| Act::Run(command.to_owned()))
}

#[cfg(test)]
pub(crate) mod tests {
    use nc_schema::keys::{Binding, Chord, Key, Leader as Chorded, LeaderPart, Modifier, Tables};

    use super::{Act, Keymap, Leader, in_menu, key_of};

    /// The keys the catalog this build carries puts in force for the
    /// development channel's default Leader.
    fn catalogs() -> Keymap {
        let catalog = nc_channel::keys::catalog_bindings();
        let defaults: Vec<(&str, Vec<Binding>)> = catalog
            .iter()
            .map(|(command, bound)| (command.as_str(), bound.clone()))
            .collect();
        Keymap::new(
            LeaderPart {
                leader: Chorded::of(Chord::parse("Alt+Super+Space").expect("a chord"))
                    .expect("a Leader"),
                chord: Modifier::Shift,
            },
            nc_schema::keys::tables(&defaults, &[]).expect("the catalog's tables"),
        )
    }

    /// A key is the vocabulary's, by its code exactly; a modifier, a lock,
    /// the numpad, Esc and any other spelling is none.
    #[test]
    fn a_key_is_one_of_the_vocabulary_by_its_code() {
        assert_eq!(key_of("KeyS"), Some(Key::KeyS));
        assert_eq!(key_of("ArrowLeft"), Some(Key::ArrowLeft));
        assert_eq!(key_of("Space"), Some(Key::Space));
        assert_eq!(key_of("Slash"), Some(Key::Slash));
        for other in [
            "",
            "s",
            "S",
            "keys",
            "KeyS ",
            "AltLeft",
            "ShiftRight",
            "MetaLeft",
            "CapsLock",
            "Numpad1",
            "Escape",
            "MediaPlayPause",
        ] {
            assert_eq!(key_of(other), None, "{other:?}");
        }
    }

    /// The leader menu is every binding of the menu's table in force, and
    /// the catalog's are Space, `s`, `h`, `d`, `q`, `/` and `w` — the
    /// workspaces' Tab and digits are not bound (Stage 17).
    #[test]
    pub(crate) fn the_menu_is_the_menu_tables_bindings_in_force() {
        let keymap = catalogs();
        let menu: Vec<(Key, String)> = keymap
            .menu()
            .into_iter()
            .map(|entry| (entry.key, entry.command))
            .collect();
        let mut expected = vec![
            (Key::Space, "sidebar.cycle".to_owned()),
            (Key::KeyS, "page.settings".to_owned()),
            (Key::KeyH, "page.help".to_owned()),
            (Key::KeyD, "page.dev".to_owned()),
            (Key::KeyQ, "page.close".to_owned()),
            (Key::Slash, "page.shortcuts".to_owned()),
            (Key::KeyW, "page.actions".to_owned()),
        ];
        expected.sort();
        let mut found = menu.clone();
        found.sort();
        assert_eq!(found, expected);
        for key in [Key::Tab, Key::Digit1, Key::Digit0, Key::KeyG, Key::KeyF] {
            assert_eq!(keymap.in_menu(key), None, "{key:?}");
        }
        assert!(keymap.is_entry("page.settings"));
        assert!(!keymap.is_entry("page.open"));
        assert!(!keymap.is_entry("page.next"));
    }

    /// With the Leader held, `q` closes the focused page and the arrows step
    /// through the pages; nothing is bound with the chord modifier held as
    /// well, and nothing else with the Leader held.
    #[test]
    fn held_the_leader_runs_q_and_the_arrows() {
        let keymap = catalogs();
        for (key, command) in [
            (Key::KeyQ, "page.close"),
            (Key::ArrowLeft, "page.previous"),
            (Key::ArrowUp, "page.previous"),
            (Key::ArrowRight, "page.next"),
            (Key::ArrowDown, "page.next"),
        ] {
            assert_eq!(keymap.held(key, false), Some(command), "{key:?}");
            assert_eq!(keymap.held(key, true), None, "{key:?} with the chord");
        }
        for key in [
            Key::Equal,
            Key::Minus,
            Key::KeyG,
            Key::KeyF,
            Key::Tab,
            Key::KeyS,
        ] {
            assert_eq!(keymap.held(key, false), None, "{key:?}");
        }
    }

    /// A person's own bindings are the ones in force: a key moved runs from
    /// its new place, and an entry unbound leaves the menu.
    #[test]
    fn the_persons_bindings_are_the_ones_in_force() {
        let catalog = nc_channel::keys::catalog_bindings();
        let defaults: Vec<(&str, Vec<Binding>)> = catalog
            .iter()
            .map(|(command, bound)| (command.as_str(), bound.clone()))
            .collect();
        let moved = vec![
            (
                "page.settings",
                vec![Binding::parse("Leader KeyO").expect("a binding")],
            ),
            ("page.dev", Vec::new()),
        ];
        let keymap = Keymap::new(
            catalogs().part(),
            nc_schema::keys::tables(&defaults, &moved).expect("tables"),
        );
        assert_eq!(keymap.in_menu(Key::KeyO), Some("page.settings"));
        assert_eq!(keymap.in_menu(Key::KeyS), None);
        assert_eq!(keymap.in_menu(Key::KeyD), None);
        assert!(!keymap.is_entry("page.dev"));
        assert_eq!(
            in_menu(Key::KeyO, &keymap),
            Some(Act::Run("page.settings".to_owned()))
        );
        assert_eq!(in_menu(Key::KeyZ, &keymap), None);
        let none = Keymap::new(catalogs().part(), Tables::default());
        assert_eq!(none.menu().len(), 0);
    }

    /// A press heard without the keyboard is a tap at once; one heard with
    /// it begins a hold, which a release with nothing pressed makes a tap.
    #[test]
    fn a_press_without_the_keyboard_is_a_tap_and_with_it_a_hold() {
        let mut leader = Leader::default();
        assert_eq!(leader.pressed(false), Some(Act::Tap));
        assert!(!leader.is_held());
        assert_eq!(leader.pressed(true), None);
        assert!(leader.is_held());
        assert_eq!(leader.pressed(true), None, "a second press holds still");
        assert_eq!(leader.up(), Some(Act::Tap));
        assert!(!leader.is_held());
        assert_eq!(leader.up(), None, "nothing is held");
        leader.down();
        assert!(leader.is_held());
        assert_eq!(leader.up(), Some(Act::Tap), "the page saw it go down");
    }

    /// A key pressed with the Leader held runs its binding at once and makes
    /// the hold no tap; a key bound nowhere runs nothing and still does; a
    /// key with nothing held runs nothing; losing the keyboard ends a hold
    /// with no tap.
    #[test]
    fn a_key_pressed_with_the_leader_held_runs_its_binding_and_is_no_tap() {
        let keymap = catalogs();
        let mut leader = Leader::default();
        assert_eq!(leader.key(Key::KeyQ, false, &keymap), None, "not held");
        leader.down();
        assert_eq!(
            leader.key(Key::KeyQ, false, &keymap),
            Some(Act::Run("page.close".to_owned()))
        );
        assert_eq!(
            leader.key(Key::ArrowRight, false, &keymap),
            Some(Act::Run("page.next".to_owned()))
        );
        assert_eq!(leader.up(), None, "a hold, not a tap");
        leader.down();
        assert_eq!(leader.key(Key::KeyZ, false, &keymap), None);
        assert_eq!(
            leader.up(),
            None,
            "a key bound nowhere still made it a hold"
        );
        leader.down();
        assert_eq!(
            leader.key(Key::KeyQ, true, &keymap),
            None,
            "the other table"
        );
        assert_eq!(leader.up(), None);
        assert_eq!(leader.pressed(true), None);
        leader.blurred();
        assert!(!leader.is_held());
        assert_eq!(leader.up(), None, "a hold the blur ended taps nothing");
    }

    /// The page is told the Leader's key by its code, which of Control, Alt,
    /// Shift and Meta it holds, where the chord modifier is, and the codes
    /// the menu binds.
    #[test]
    fn the_page_is_told_the_leader_and_the_menus_codes() {
        let told = catalogs().told().to_canonical();
        assert!(told.contains("\"code\":\"Space\""), "{told}");
        assert!(told.contains("\"mods\":[false,true,false,true]"), "{told}");
        assert!(told.contains("\"chord\":2"), "{told}");
        for code in ["Space", "KeyS", "KeyH", "KeyD", "KeyQ", "Slash", "KeyW"] {
            assert!(told.contains(&format!("\"{code}\"")), "{code}: {told}");
        }
        assert!(
            !told.contains("ArrowLeft"),
            "a held key is not the menu's: {told}"
        );
    }
}

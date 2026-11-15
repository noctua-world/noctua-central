//! The Leader's grammar: the keys a binding names, and the judge a choice of
//! keys answers to.
//!
//! `NOCTUA.md §12.8` gives each channel a Leader — a chord held to reach the
//! interface from anywhere — and binds commands to keys pressed with it. This
//! module is the one rule for every reader of a binding: the catalog compiler
//! reads each binding the catalog declares through [`Binding::parse`], and a
//! person's choice of keys is judged by [`intra`], [`settle`], [`tables`] and
//! [`warnings`] before anything binds a key.
//!
//! # The grammar
//!
//! ```text
//! chord    = *(modifier "+") key      each modifier once, in the order Control, Alt, Shift, Super
//! binding  = "Leader " key            a key in the leader menu
//!          / "Leader+" key            a key with the Leader held
//!          / "Leader+Chord+" key      a key with the Leader and the chord modifier held
//! modifier = "Control" / "Alt" / "Shift" / "Super"
//! key      = a name of Key::ALL, as KeyboardEvent.code writes it
//! ```
//!
//! A `+` is held together and a space is then, as `Ctrl+Alt+Space` and
//! `Leader s` are written in the documents. **`Chord` names a role, not a
//! key**, so a binding holds whatever modifier a person makes the chord
//! modifier. **A key is a physical key**, named as the W3C's
//! `KeyboardEvent.code` names it — `KeyS`, `Digit1`, `Equal`, `ArrowLeft` —
//! so neither Shift nor a keyboard layout turns one key into another, and the
//! interface's webview receives the same names. The vocabulary is closed:
//! the modifiers, the lock keys, the numpad, the media keys and `Escape`,
//! which `§12.8` gives to the drawer, are not in it.
//!
//! **One spelling, and every other refused.** A text [`Binding::parse`]
//! accepts is the text [`Binding::written`] writes back, and the same holds
//! of [`Chord`], so a binding is compared as it is written and never
//! normalised: lower case, an alias such as `s`, an Emacs chord, a modifier
//! written inside a binding, two keys and a doubled space are each refused.
//!
//! # The judge
//!
//! A channel binds its keys under a [`LeaderPart`], its Leader and its chord
//! modifier, and `§12.8` refuses a configuration that collides:
//!
//! 1. no Leader holds its own chord modifier — [`intra`];
//! 2. a Leader held with its chord modifier is never another channel's
//!    Leader, and 3. no two channels share a Leader — [`apart`], which
//!    [`settle`] asks of every pair that is not two defaults;
//! 4. no two bindings in one table share a key — [`tables`].
//!
//! A Leader holds at least one modifier besides, which [`Leader::of`]
//! refuses to make otherwise: the Leader is a chord (`§12.8`), and a bare key
//! taken for the whole computer is taken from every program on it.
//!
//! **Defaults meeting defaults are not a collision**: two development
//! checkouts share their channel's Leader until one of them is configured
//! (`NOCTUA.md §17.1`), and [`settle`] judges only a configured part.
//!
//! **A key the system uses is accepted with a warning** that names the
//! system (`§12.8`), and so is a held key no hand can press while holding the
//! Leader: [`warnings`] reports both, and refuses nothing.
//!
//! # What this answers (`§2.4`)
//!
//! **A text from outside is repeated escaped and cut short.** A [`Malformed`]
//! quotes what it read the way every diagnostic of this crate does, because a
//! binding is its author's text, and a person's configuration is read through
//! the same parser.
//!
//! **The vocabulary is closed and the grammar has one spelling**, so a binding
//! names one physical key in one table and nothing else: it carries no
//! argument, and the command it runs is a name the caller holds.
//!
//! **Nothing here reads or writes anything.** The judge is handed every part
//! and every binding it weighs, so what it answers depends on nothing but
//! them; which files are read, and whose, is `nc-channel`'s.
//!
//! **What this does not answer**: whether [`SYSTEM_SHORTCUTS`] is every
//! shortcut a system keeps — it is what its sources write, no more — and
//! whether a Leader takes a chord a program relies on, which no table here
//! could list.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::excerpt;

/// A key held down with another (`NOCTUA.md §12.8`).
///
/// **A physical key, never a character**: the same key is Super on Linux, Win
/// on Windows and Command on a Mac. The variants are declared in the order a
/// chord writes them, which is Apple's ⌃⌥⇧⌘ and the order every Leader in
/// the table of `§12.8` is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Modifier {
    /// Ctrl, or Control on a Mac.
    Control,
    /// Alt, or Option on a Mac.
    Alt,
    /// Shift, on every keyboard.
    Shift,
    /// Super, Win on Windows, or Command on a Mac.
    Super,
}

impl Modifier {
    /// Every modifier, in the order a chord writes them.
    pub const ALL: [Self; 4] = [Self::Control, Self::Alt, Self::Shift, Self::Super];

    /// The word the grammar writes this modifier as.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Control => "Control",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Super => "Super",
        }
    }

    /// The modifier the grammar writes as `word`, if any.
    #[must_use]
    pub fn of_word(word: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|modifier| modifier.word() == word)
    }

    /// How `keyboard` labels this key.
    #[must_use]
    pub const fn written_for(self, keyboard: Keyboard) -> &'static str {
        match (self, keyboard) {
            (Self::Control, Keyboard::Mac) => "Control",
            (Self::Control, Keyboard::Linux | Keyboard::Windows) => "Ctrl",
            (Self::Alt, Keyboard::Mac) => "Option",
            (Self::Alt, Keyboard::Linux | Keyboard::Windows) => "Alt",
            (Self::Shift, Keyboard::Linux | Keyboard::Windows | Keyboard::Mac) => "Shift",
            (Self::Super, Keyboard::Mac) => "Command",
            (Self::Super, Keyboard::Windows) => "Win",
            (Self::Super, Keyboard::Linux) => "Super",
        }
    }

    /// This modifier's place in a [`Modifiers`] set.
    const fn bit(self) -> u8 {
        match self {
            Self::Control => 1,
            Self::Alt => 2,
            Self::Shift => 4,
            Self::Super => 8,
        }
    }
}

/// Whose labels a chord is written in.
///
/// **Named by the caller, never asked of the system this runs on**, so the
/// three spellings are one table every build can test (`NOCTUA.md §2.7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Keyboard {
    /// A Linux keyboard.
    Linux,
    /// A Windows keyboard.
    Windows,
    /// A Mac keyboard.
    Mac,
}

impl Keyboard {
    /// Every keyboard, in the order the documents write them.
    pub const ALL: [Self; 3] = [Self::Linux, Self::Windows, Self::Mac];
}

/// Modifiers held together: a set, which a chord writes in one order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Modifiers(u8);

impl Modifiers {
    /// No modifier at all.
    pub const NONE: Self = Self(0);

    /// The set holding each of `modifiers`, once however often it is named.
    #[must_use]
    pub const fn of(modifiers: &[Modifier]) -> Self {
        let mut bits = 0;
        let mut at = 0;
        while at < modifiers.len() {
            bits |= modifiers[at].bit();
            at += 1;
        }
        Self(bits)
    }

    /// This set with `modifier` held as well.
    #[must_use]
    pub const fn with(self, modifier: Modifier) -> Self {
        Self(self.0 | modifier.bit())
    }

    /// Whether `modifier` is held.
    #[must_use]
    pub const fn holds(self, modifier: Modifier) -> bool {
        self.0 & modifier.bit() != 0
    }

    /// Whether nothing is held.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The modifiers held, in the order a chord writes them.
    pub fn iter(self) -> impl Iterator<Item = Modifier> {
        Modifier::ALL
            .into_iter()
            .filter(move |modifier| self.holds(*modifier))
    }
}

/// A physical key, named as the W3C's `KeyboardEvent.code` names it.
///
/// **A closed set**: the letters, the digits of the top row, the punctuation
/// of a US board, Space, Tab, Enter and Backspace, the arrows, the six keys
/// above them and the function keys to F12. The modifiers, the lock keys,
/// `Fn`, the `Intl` keys, the numpad, the media keys and `Escape` are not in
/// it. A key joins it as a variant here and a row of `VOCABULARY`, in the
/// same place in both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// The A key.
    KeyA,
    /// The B key.
    KeyB,
    /// The C key.
    KeyC,
    /// The D key.
    KeyD,
    /// The E key.
    KeyE,
    /// The F key.
    KeyF,
    /// The G key.
    KeyG,
    /// The H key.
    KeyH,
    /// The I key.
    KeyI,
    /// The J key.
    KeyJ,
    /// The K key.
    KeyK,
    /// The L key.
    KeyL,
    /// The M key.
    KeyM,
    /// The N key.
    KeyN,
    /// The O key.
    KeyO,
    /// The P key.
    KeyP,
    /// The Q key.
    KeyQ,
    /// The R key.
    KeyR,
    /// The S key.
    KeyS,
    /// The T key.
    KeyT,
    /// The U key.
    KeyU,
    /// The V key.
    KeyV,
    /// The W key.
    KeyW,
    /// The X key.
    KeyX,
    /// The Y key.
    KeyY,
    /// The Z key.
    KeyZ,
    /// The 0 of the top row.
    Digit0,
    /// The 1 of the top row.
    Digit1,
    /// The 2 of the top row.
    Digit2,
    /// The 3 of the top row.
    Digit3,
    /// The 4 of the top row.
    Digit4,
    /// The 5 of the top row.
    Digit5,
    /// The 6 of the top row.
    Digit6,
    /// The 7 of the top row.
    Digit7,
    /// The 8 of the top row.
    Digit8,
    /// The 9 of the top row.
    Digit9,
    /// The `-` key.
    Minus,
    /// The `=` key.
    Equal,
    /// The `[` key.
    BracketLeft,
    /// The `]` key.
    BracketRight,
    /// The `\` key.
    Backslash,
    /// The `;` key.
    Semicolon,
    /// The `'` key.
    Quote,
    /// The key left of 1, `` ` `` on a US board.
    Backquote,
    /// The `,` key.
    Comma,
    /// The `.` key.
    Period,
    /// The `/` key.
    Slash,
    /// The space bar.
    Space,
    /// The Tab key.
    Tab,
    /// Enter, or Return on a Mac.
    Enter,
    /// Backspace, or Delete on a Mac.
    Backspace,
    /// The up arrow.
    ArrowUp,
    /// The down arrow.
    ArrowDown,
    /// The left arrow.
    ArrowLeft,
    /// The right arrow.
    ArrowRight,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Insert.
    Insert,
    /// Delete, or Forward Delete on a Mac.
    Delete,
    /// F1.
    F1,
    /// F2.
    F2,
    /// F3.
    F3,
    /// F4.
    F4,
    /// F5.
    F5,
    /// F6.
    F6,
    /// F7.
    F7,
    /// F8.
    F8,
    /// F9.
    F9,
    /// F10.
    F10,
    /// F11.
    F11,
    /// F12.
    F12,
}

/// Every key of the vocabulary: its variant, its `KeyboardEvent.code` name and
/// how a Linux or a Windows keyboard labels it, in [`Key`]'s order.
///
/// **One table**, so a key's name and its label are never two lists that
/// disagree: [`Key::ALL`], [`Key::code`], [`Key::of_code`] and
/// [`Key::written_for`] all read it, and a row out of [`Key`]'s order is a key
/// named as another — which the round trip in this module's tests refuses.
const VOCABULARY: [(Key, &str, &str); 73] = [
    (Key::KeyA, "KeyA", "A"),
    (Key::KeyB, "KeyB", "B"),
    (Key::KeyC, "KeyC", "C"),
    (Key::KeyD, "KeyD", "D"),
    (Key::KeyE, "KeyE", "E"),
    (Key::KeyF, "KeyF", "F"),
    (Key::KeyG, "KeyG", "G"),
    (Key::KeyH, "KeyH", "H"),
    (Key::KeyI, "KeyI", "I"),
    (Key::KeyJ, "KeyJ", "J"),
    (Key::KeyK, "KeyK", "K"),
    (Key::KeyL, "KeyL", "L"),
    (Key::KeyM, "KeyM", "M"),
    (Key::KeyN, "KeyN", "N"),
    (Key::KeyO, "KeyO", "O"),
    (Key::KeyP, "KeyP", "P"),
    (Key::KeyQ, "KeyQ", "Q"),
    (Key::KeyR, "KeyR", "R"),
    (Key::KeyS, "KeyS", "S"),
    (Key::KeyT, "KeyT", "T"),
    (Key::KeyU, "KeyU", "U"),
    (Key::KeyV, "KeyV", "V"),
    (Key::KeyW, "KeyW", "W"),
    (Key::KeyX, "KeyX", "X"),
    (Key::KeyY, "KeyY", "Y"),
    (Key::KeyZ, "KeyZ", "Z"),
    (Key::Digit0, "Digit0", "0"),
    (Key::Digit1, "Digit1", "1"),
    (Key::Digit2, "Digit2", "2"),
    (Key::Digit3, "Digit3", "3"),
    (Key::Digit4, "Digit4", "4"),
    (Key::Digit5, "Digit5", "5"),
    (Key::Digit6, "Digit6", "6"),
    (Key::Digit7, "Digit7", "7"),
    (Key::Digit8, "Digit8", "8"),
    (Key::Digit9, "Digit9", "9"),
    (Key::Minus, "Minus", "-"),
    (Key::Equal, "Equal", "="),
    (Key::BracketLeft, "BracketLeft", "["),
    (Key::BracketRight, "BracketRight", "]"),
    (Key::Backslash, "Backslash", "\\"),
    (Key::Semicolon, "Semicolon", ";"),
    (Key::Quote, "Quote", "'"),
    (Key::Backquote, "Backquote", "`"),
    (Key::Comma, "Comma", ","),
    (Key::Period, "Period", "."),
    (Key::Slash, "Slash", "/"),
    (Key::Space, "Space", "Space"),
    (Key::Tab, "Tab", "Tab"),
    (Key::Enter, "Enter", "Enter"),
    (Key::Backspace, "Backspace", "Backspace"),
    (Key::ArrowUp, "ArrowUp", "Up"),
    (Key::ArrowDown, "ArrowDown", "Down"),
    (Key::ArrowLeft, "ArrowLeft", "Left"),
    (Key::ArrowRight, "ArrowRight", "Right"),
    (Key::Home, "Home", "Home"),
    (Key::End, "End", "End"),
    (Key::PageUp, "PageUp", "PageUp"),
    (Key::PageDown, "PageDown", "PageDown"),
    (Key::Insert, "Insert", "Insert"),
    (Key::Delete, "Delete", "Delete"),
    (Key::F1, "F1", "F1"),
    (Key::F2, "F2", "F2"),
    (Key::F3, "F3", "F3"),
    (Key::F4, "F4", "F4"),
    (Key::F5, "F5", "F5"),
    (Key::F6, "F6", "F6"),
    (Key::F7, "F7", "F7"),
    (Key::F8, "F8", "F8"),
    (Key::F9, "F9", "F9"),
    (Key::F10, "F10", "F10"),
    (Key::F11, "F11", "F11"),
    (Key::F12, "F12", "F12"),
];

impl Key {
    /// Every key of the vocabulary, in the order [`Key`] declares them.
    pub const ALL: [Self; 73] = {
        let mut keys = [Self::KeyA; 73];
        let mut at = 0;
        while at < VOCABULARY.len() {
            keys[at] = VOCABULARY[at].0;
            at += 1;
        }
        keys
    };

    /// The name `KeyboardEvent.code` gives this key, which is how the grammar
    /// writes it.
    #[must_use]
    pub const fn code(self) -> &'static str {
        VOCABULARY[self as usize].1
    }

    /// The key `KeyboardEvent.code` names `code`, if the vocabulary holds it.
    ///
    /// **The name exactly**: `keys`, `KEYS` and `S` are no key, so the
    /// vocabulary has one spelling of each, as the grammar does.
    #[must_use]
    pub fn of_code(code: &str) -> Option<Self> {
        VOCABULARY
            .iter()
            .find(|(_, name, _)| *name == code)
            .map(|(key, _, _)| *key)
    }

    /// How `keyboard` labels this key: its position on a US board, and a Mac's
    /// own names for the three keys it calls otherwise.
    #[must_use]
    pub const fn written_for(self, keyboard: Keyboard) -> &'static str {
        match (self, keyboard) {
            (Self::Enter, Keyboard::Mac) => "Return",
            (Self::Backspace, Keyboard::Mac) => "Delete",
            (Self::Delete, Keyboard::Mac) => "Forward Delete",
            _ => VOCABULARY[self as usize].2,
        }
    }
}

/// Modifiers held and one key pressed with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Chord {
    /// The modifiers held.
    modifiers: Modifiers,
    /// The key pressed with them.
    key: Key,
}

impl Chord {
    /// `key` pressed with `modifiers` held.
    #[must_use]
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// A chord as the grammar writes it: `Control+Alt+Space`.
    ///
    /// # Errors
    ///
    /// [`Malformed::NotAModifier`] for a word before the key that names no
    /// modifier, [`Malformed::OutOfOrder`] for a modifier named twice or out
    /// of the order [`Modifier::ALL`] holds, and [`Malformed::NotAKey`] when
    /// what ends the text is no key of the vocabulary — an empty text among
    /// them.
    pub fn parse(text: &str) -> Result<Self, Malformed> {
        let (held, key) = match text.rsplit_once('+') {
            Some((held, key)) => (Some(held), key),
            None => (None, text),
        };
        let mut modifiers = Modifiers::NONE;
        let mut last: Option<Modifier> = None;
        for word in held.into_iter().flat_map(|held| held.split('+')) {
            let Some(modifier) = Modifier::of_word(word) else {
                return Err(Malformed::NotAModifier {
                    written: excerpt(text),
                    word: excerpt(word),
                });
            };
            if last.is_some_and(|before| before >= modifier) {
                return Err(Malformed::OutOfOrder(excerpt(text)));
            }
            last = Some(modifier);
            modifiers = modifiers.with(modifier);
        }
        let Some(key) = Key::of_code(key) else {
            return Err(Malformed::NotAKey {
                written: excerpt(text),
                key: excerpt(key),
            });
        };
        Ok(Self::new(modifiers, key))
    }

    /// The modifiers held.
    #[must_use]
    pub const fn modifiers(self) -> Modifiers {
        self.modifiers
    }

    /// The key pressed with them.
    #[must_use]
    pub const fn key(self) -> Key {
        self.key
    }

    /// The chord as the grammar writes it, which [`Chord::parse`] reads back.
    #[must_use]
    pub fn written(self) -> String {
        let mut words: Vec<&str> = self.modifiers.iter().map(Modifier::word).collect();
        words.push(self.key.code());
        words.join("+")
    }

    /// The chord as `keyboard` labels it: `Ctrl+Alt+Space`, `Option+Space`.
    #[must_use]
    pub fn written_for(self, keyboard: Keyboard) -> String {
        let mut labels: Vec<&str> = self
            .modifiers
            .iter()
            .map(|modifier| modifier.written_for(keyboard))
            .collect();
        labels.push(self.key.written_for(keyboard));
        labels.join("+")
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.written())
    }
}

/// A chord fit to be a channel's Leader: one that holds a modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Leader(Chord);

impl Leader {
    /// `chord` as a Leader.
    ///
    /// **`const`, so a Leader written in code is judged when it compiles**: a
    /// channel's default is a `const` item built through this, and a default
    /// that broke the rule would be a build that fails.
    ///
    /// # Errors
    ///
    /// [`LeaderRule::WithoutAModifier`] when `chord` holds none: a bare key
    /// taken for the whole computer is taken from every program on it.
    pub const fn of(chord: Chord) -> Result<Self, LeaderRule> {
        if chord.modifiers.is_empty() {
            Err(LeaderRule::WithoutAModifier)
        } else {
            Ok(Self(chord))
        }
    }

    /// The chord the Leader is.
    #[must_use]
    pub const fn chord(self) -> Chord {
        self.0
    }

    /// The keys held with the Leader's key, in the order a chord writes them.
    #[must_use]
    pub fn modifiers(self) -> Vec<Modifier> {
        self.0.modifiers.iter().collect()
    }

    /// The Leader's own key: Space, for every channel's default.
    #[must_use]
    pub const fn key(self) -> Key {
        self.0.key
    }

    /// The chord pressed for the Leader held with `chord`, the chord modifier.
    #[must_use]
    pub const fn with(self, chord: Modifier) -> Chord {
        Chord::new(self.0.modifiers.with(chord), self.0.key)
    }

    /// The Leader as the grammar writes it: `Alt+Space`.
    #[must_use]
    pub fn written(self) -> String {
        self.0.written()
    }

    /// The Leader as `keyboard` labels it: `Ctrl+Alt+Space`, `Option+Space`.
    #[must_use]
    pub fn written_for(self, keyboard: Keyboard) -> String {
        self.0.written_for(keyboard)
    }
}

/// What a channel binds its keys under: its Leader and its chord modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeaderPart {
    /// The chord held to reach the interface.
    pub leader: Leader,
    /// The modifier a binding's `Chord` names, held with the Leader.
    pub chord: Modifier,
}

/// A rule of `NOCTUA.md §12.8` a Leader breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeaderRule {
    /// The chord holds no modifier, so it is a bare key.
    WithoutAModifier,
    /// The Leader holds its own chord modifier, rule 1.
    HoldsTheChordModifier,
    /// The Leader held with its chord modifier is another channel's Leader,
    /// or another's held with its chord is this one, rule 2.
    HeldWithTheChordIsAnothers,
    /// Another channel has this Leader, rule 3.
    SharedWithAnother,
}

impl fmt::Display for LeaderRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::WithoutAModifier => {
                "a Leader holds at least one modifier, and this one is a bare key"
            }
            Self::HoldsTheChordModifier => {
                "a Leader never holds its own chord modifier, or a key held with the Leader \
                 and one held with the chord as well would be one chord"
            }
            Self::HeldWithTheChordIsAnothers => {
                "a Leader held with its chord modifier is never another channel's Leader, or \
                 a key pressed under one would reach the other"
            }
            Self::SharedWithAnother => {
                "no two channels share a Leader, or one chord would reach two interfaces"
            }
        })
    }
}

/// Whether `part` keeps the rule that needs no other channel: its Leader does
/// not hold its chord modifier (rule 1).
///
/// # Errors
///
/// [`LeaderRule::HoldsTheChordModifier`] when it does: the Leader held and the
/// Leader held with its chord would be one chord, so the two tables of held
/// keys would be one.
pub const fn intra(part: LeaderPart) -> Result<(), LeaderRule> {
    if part.leader.0.modifiers.holds(part.chord) {
        Err(LeaderRule::HoldsTheChordModifier)
    } else {
        Ok(())
    }
}

/// Whether two channels' parts keep the rules between channels (rules 2
/// and 3).
///
/// # Errors
///
/// [`LeaderRule::SharedWithAnother`] when the two Leaders are one chord, and
/// [`LeaderRule::HeldWithTheChordIsAnothers`] when either Leader held with its
/// own chord modifier is the other's Leader.
pub fn apart(one: LeaderPart, other: LeaderPart) -> Result<(), LeaderRule> {
    if one.leader == other.leader {
        return Err(LeaderRule::SharedWithAnother);
    }
    if one.leader.with(one.chord) == other.leader.0
        || other.leader.with(other.chord) == one.leader.0
    {
        return Err(LeaderRule::HeldWithTheChordIsAnothers);
    }
    Ok(())
}

/// One channel's part in a settlement: its default, and what it is configured
/// with, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claim<R> {
    /// Where the channel is settled: lower first.
    pub rank: R,
    /// The channel's default part.
    pub default: LeaderPart,
    /// The part its configuration names, if it names one.
    pub configured: Option<LeaderPart>,
}

/// How one claim was settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict<R> {
    /// It claimed nothing but its default.
    Default,
    /// Its configured part breaks no rule, and is in force.
    Accepted,
    /// Its configured part breaks `rule` — against the claim ranked `by`, or
    /// alone where that is `None` — and its default is in force.
    Refused {
        /// The rule the configured part breaks.
        rule: LeaderRule,
        /// The rank of the claim it collides with.
        by: Option<R>,
    },
}

/// A claim settled: the part in force, and why it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settled<R> {
    /// The Leader and chord modifier the channel binds under.
    pub part: LeaderPart,
    /// Whether its configuration was accepted, refused or absent.
    pub verdict: Verdict<R>,
}

/// Every claim settled, in the order `claims` lists them (`NOCTUA.md §17.1`).
///
/// **In rank order, whatever order they are listed in.** A configured part is
/// accepted only if it keeps [`intra`], and [`apart`] from the part settled
/// for every claim ranked before it and from the default of every claim
/// ranked after it; a part refused leaves its default in force. So what a
/// claim settles to depends on the claims before it and the defaults after
/// it alone — a channel is never refused for what one after it configured —
/// and two claims both on their defaults are never judged against each other,
/// which is what lets two development checkouts share their Leader until one
/// of them is configured. Ranks are distinct; two equal ones are settled in
/// the order listed.
#[must_use]
pub fn settle<R: Ord + Copy>(claims: &[Claim<R>]) -> Vec<Settled<R>> {
    let mut order: Vec<usize> = (0..claims.len()).collect();
    order.sort_by_key(|at| claims[*at].rank);
    let mut in_rank: Vec<(usize, Settled<R>)> = Vec::with_capacity(claims.len());
    for (place, &at) in order.iter().enumerate() {
        let claim = claims[at];
        let verdict = match claim.configured {
            None => Verdict::Default,
            Some(part) => {
                let before = in_rank
                    .iter()
                    .map(|(other, held)| (claims[*other].rank, held.part));
                let after = order[place + 1..]
                    .iter()
                    .map(|&other| (claims[other].rank, claims[other].default));
                match intra(part) {
                    Err(rule) => Verdict::Refused { rule, by: None },
                    Ok(()) => before
                        .chain(after)
                        .find_map(|(rank, other)| {
                            apart(part, other).err().map(|rule| Verdict::Refused {
                                rule,
                                by: Some(rank),
                            })
                        })
                        .unwrap_or(Verdict::Accepted),
                }
            }
        };
        let part = match (verdict, claim.configured) {
            (Verdict::Accepted, Some(part)) => part,
            _ => claim.default,
        };
        in_rank.push((at, Settled { part, verdict }));
    }
    in_rank.sort_by_key(|(at, _)| *at);
    in_rank.into_iter().map(|(_, settled)| settled).collect()
}

/// Which of the three tables a binding is in (`NOCTUA.md §12.8`, `§12.9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Table {
    /// A key in the leader menu: the Leader, then the key.
    Menu,
    /// A key pressed with the Leader held.
    Held,
    /// A key pressed with the Leader and the chord modifier held.
    HeldWithChord,
}

impl Table {
    /// Every table, in the order the documents write them.
    pub const ALL: [Self; 3] = [Self::Menu, Self::Held, Self::HeldWithChord];

    /// What a binding in this table opens with, before its key.
    #[must_use]
    pub const fn opening(self) -> &'static str {
        match self {
            Self::Menu => "Leader ",
            Self::Held => "Leader+",
            Self::HeldWithChord => "Leader+Chord+",
        }
    }
}

/// A key a command is bound to, in one table.
///
/// **The same key in two tables is two bindings**: one pressed in the leader
/// menu and one pressed with the Leader held are different presses, so each
/// may run a different command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Binding {
    /// The table it is in.
    pub table: Table,
    /// The key pressed.
    pub key: Key,
}

impl Binding {
    /// A binding as the grammar writes it, `Leader+Chord+KeyQ` among them.
    ///
    /// # Errors
    ///
    /// [`Malformed::NotAForm`] for a text that opens with none of the three
    /// forms, and [`Malformed::NotAKey`] for one whose form is followed by
    /// anything but one key of the vocabulary, spelled exactly.
    pub fn parse(text: &str) -> Result<Self, Malformed> {
        // **The longest opening first**: `Leader+Chord+` opens with `Leader+`
        // too, and read the other way round it would leave `Chord+KeyQ` where
        // one key belongs.
        for table in [Table::HeldWithChord, Table::Held, Table::Menu] {
            if let Some(key) = text.strip_prefix(table.opening()) {
                return Key::of_code(key)
                    .map(|key| Self { table, key })
                    .ok_or_else(|| Malformed::NotAKey {
                        written: excerpt(text),
                        key: excerpt(key),
                    });
            }
        }
        Err(Malformed::NotAForm(excerpt(text)))
    }

    /// The binding as the grammar writes it, which [`Binding::parse`] reads
    /// back.
    #[must_use]
    pub fn written(self) -> String {
        format!("{}{}", self.table.opening(), self.key.code())
    }

    /// The keys pressed for this binding under `part`, as `keyboard` labels
    /// them: `Alt+Space S`, `Alt+Space+Q`, `Alt+Shift+Space+Q`.
    #[must_use]
    pub fn written_for(self, part: LeaderPart, keyboard: Keyboard) -> String {
        let key = self.key.written_for(keyboard);
        match self.table {
            Table::Menu => format!("{} {key}", part.leader.written_for(keyboard)),
            Table::Held => format!("{}+{key}", part.leader.written_for(keyboard)),
            Table::HeldWithChord => {
                format!(
                    "{}+{key}",
                    part.leader.with(part.chord).written_for(keyboard)
                )
            }
        }
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.written())
    }
}

/// A text the grammar does not read, and why.
///
/// Each variant holds what it repeats of the text the way every diagnostic of
/// this crate repeats a name — escaped and cut short — because a binding's text
/// is its author's, and a person's configuration is read through the same
/// parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Malformed {
    /// A binding that opens with none of the three forms.
    NotAForm(String),
    /// What stands where one key belongs is no key of the vocabulary.
    NotAKey {
        /// The text read.
        written: String,
        /// What stands where the key belongs.
        key: String,
    },
    /// What stands where a modifier belongs is no modifier.
    NotAModifier {
        /// The text read.
        written: String,
        /// What stands where the modifier belongs.
        word: String,
    },
    /// A chord that names a modifier twice or out of order.
    OutOfOrder(String),
}

impl fmt::Display for Malformed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAForm(written) => write!(
                formatter,
                "`{written}` is none of the three forms, `Leader <key>`, `Leader+<key>` and \
                 `Leader+Chord+<key>`"
            ),
            Self::NotAKey { written, key } => write!(
                formatter,
                "`{written}` holds `{key}` where one key belongs, and no key is named that \
                 as `KeyboardEvent.code` names keys"
            ),
            Self::NotAModifier { written, word } => write!(
                formatter,
                "`{written}` holds `{word}` where a modifier belongs, and it is none of \
                 `Control`, `Alt`, `Shift` and `Super`"
            ),
            Self::OutOfOrder(written) => write!(
                formatter,
                "`{written}` names a modifier twice or out of the order `Control`, `Alt`, \
                 `Shift`, `Super`"
            ),
        }
    }
}

impl std::error::Error for Malformed {}

/// Every binding in force, each beside the one command it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tables {
    /// The command each binding runs.
    bound: BTreeMap<Binding, String>,
}

impl Tables {
    /// The command `binding` runs, if it is bound.
    #[must_use]
    pub fn command(&self, binding: Binding) -> Option<&str> {
        self.bound.get(&binding).map(String::as_str)
    }

    /// Every binding and its command, table by table and key by key.
    pub fn iter(&self) -> impl Iterator<Item = (Binding, &str)> {
        self.bound
            .iter()
            .map(|(binding, command)| (*binding, command.as_str()))
    }

    /// How many bindings are in force.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bound.len()
    }

    /// Whether no binding is in force.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bound.is_empty()
    }
}

/// One key of one table bound twice, rule 4: to two commands, or to one
/// twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBoundTwice {
    /// The binding claimed twice.
    pub binding: Binding,
    /// The command that claimed it first.
    pub first: String,
    /// The command that claimed it again.
    pub second: String,
}

impl fmt::Display for KeyBoundTwice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "`{}` and `{}` are both bound to `{}`",
            self.second, self.first, self.binding
        )
    }
}

/// The tables `defaults` and `chosen` make together (rule 4).
///
/// A command `chosen` names is bound to the keys it names there in place of
/// its defaults — `[]` unbinds it — and one only `chosen` names is bound as
/// well, after every default; a command named twice in either is read where it
/// is named first.
///
/// # Errors
///
/// Every [`KeyBoundTwice`], in the order the commands are read: one binding
/// claimed by two commands, or twice by one.
pub fn tables(
    defaults: &[(&str, Vec<Binding>)],
    chosen: &[(&str, Vec<Binding>)],
) -> Result<Tables, Vec<KeyBoundTwice>> {
    let mut picked: BTreeMap<&str, &[Binding]> = BTreeMap::new();
    for (command, bindings) in chosen {
        picked.entry(*command).or_insert(bindings.as_slice());
    }
    let mut read: BTreeSet<&str> = BTreeSet::new();
    let mut in_force: Vec<(&str, &[Binding])> = Vec::new();
    for (command, bindings) in defaults.iter().chain(chosen) {
        if read.insert(*command) {
            let bindings = picked.get(command).copied().unwrap_or(bindings.as_slice());
            in_force.push((*command, bindings));
        }
    }
    let mut bound: BTreeMap<Binding, String> = BTreeMap::new();
    let mut twice = Vec::new();
    for (command, bindings) in in_force {
        for &binding in bindings {
            match bound.get(&binding) {
                Some(first) => twice.push(KeyBoundTwice {
                    binding,
                    first: first.clone(),
                    second: command.to_owned(),
                }),
                None => {
                    bound.insert(binding, command.to_owned());
                }
            }
        }
    }
    if twice.is_empty() {
        Ok(Tables { bound })
    } else {
        Err(twice)
    }
}

/// What a choice of keys is warned of, and never refused for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warning {
    /// A system uses the chord pressed, the Leader's own where no binding is
    /// named.
    TheSystemUses {
        /// The binding pressed, or `None` for the Leader itself.
        binding: Option<Binding>,
        /// The shortcut the system keeps on that chord.
        shortcut: &'static SystemShortcut,
    },
    /// A held binding on the Leader's own key, which no hand presses again
    /// while holding it.
    Unpressable {
        /// The binding nobody can press.
        binding: Binding,
    },
}

/// Every warning `part` and `tables` raise, the Leader's first and then each
/// binding's in table order.
///
/// **A system shortcut is matched exactly**, against the chord a person
/// presses: the Leader itself; for a held binding, the Leader's modifiers and
/// the key; for one held with the chord, the chord modifier as well. A key in
/// the leader menu is pressed after the Leader is let go, inside the
/// interface, and no system shortcut reaches it.
#[must_use]
pub fn warnings(part: LeaderPart, tables: &Tables) -> Vec<Warning> {
    let uses = |chord: Chord| {
        SYSTEM_SHORTCUTS
            .iter()
            .filter(move |shortcut| shortcut.chord == chord)
    };
    let mut found: Vec<Warning> = uses(part.leader.0)
        .map(|shortcut| Warning::TheSystemUses {
            binding: None,
            shortcut,
        })
        .collect();
    for (binding, _) in tables.iter() {
        let held = match binding.table {
            Table::Menu => continue,
            Table::Held => part.leader.0.modifiers,
            Table::HeldWithChord => part.leader.0.modifiers.with(part.chord),
        };
        found.extend(
            uses(Chord::new(held, binding.key)).map(|shortcut| Warning::TheSystemUses {
                binding: Some(binding),
                shortcut,
            }),
        );
        if binding.key == part.leader.0.key {
            found.push(Warning::Unpressable { binding });
        }
    }
    found
}

/// A chord a system keeps for itself, and the source that says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemShortcut {
    /// The keyboard the system is used on.
    pub keyboard: Keyboard,
    /// The system, named as its source names it.
    pub system: &'static str,
    /// The chord it keeps.
    pub chord: Chord,
    /// The page or file that says so, as fetched.
    pub source: &'static str,
}

/// Where Microsoft lists Windows' keyboard shortcuts.
const MICROSOFT: &str = "https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec";

/// Where Apple lists the Mac's keyboard shortcuts.
const APPLE: &str = "https://support.apple.com/en-us/102650";

/// GNOME's window manager's default keybindings, in its settings schema.
const GNOME: &str = "https://raw.githubusercontent.com/GNOME/gsettings-desktop-schemas/master/schemas/org.gnome.desktop.wm.keybindings.gschema.xml.in";

/// KDE Plasma's search launcher, and the shortcuts it is opened with.
const KRUNNER: &str = "https://raw.githubusercontent.com/KDE/plasma-workspace/master/krunner/org.kde.krunner.desktop.cmake";

/// KDE Plasma's window switcher, and the shortcuts it walks windows with.
const KWIN_TABBOX: &str = "https://raw.githubusercontent.com/KDE/kwin/master/src/tabbox/tabbox.cpp";

/// KDE Plasma's window actions, and the shortcuts each is reached by.
const KWIN_USERACTIONS: &str =
    "https://raw.githubusercontent.com/KDE/kwin/master/src/useractions.cpp";

/// One row of [`SYSTEM_SHORTCUTS`]: `key` with `held`, as `source` says.
const fn kept(
    keyboard: Keyboard,
    system: &'static str,
    source: &'static str,
    held: &[Modifier],
    key: Key,
) -> SystemShortcut {
    SystemShortcut {
        keyboard,
        system,
        chord: Chord::new(Modifiers::of(held), key),
        source,
    }
}

/// A chord Windows keeps, as Microsoft lists it.
const fn windows(held: &[Modifier], key: Key) -> SystemShortcut {
    kept(Keyboard::Windows, "Windows", MICROSOFT, held, key)
}

/// A chord macOS keeps, as Apple lists it.
const fn mac(held: &[Modifier], key: Key) -> SystemShortcut {
    kept(Keyboard::Mac, "macOS", APPLE, held, key)
}

/// A chord GNOME keeps by default, as its settings schema writes it.
const fn gnome(held: &[Modifier], key: Key) -> SystemShortcut {
    kept(Keyboard::Linux, "GNOME", GNOME, held, key)
}

/// A chord KDE Plasma keeps by default, as `source` writes it.
const fn kde(source: &'static str, held: &[Modifier], key: Key) -> SystemShortcut {
    kept(Keyboard::Linux, "KDE Plasma", source, held, key)
}

/// Every chord a system keeps that a Leader or a binding might press, each
/// from the source that says so.
///
/// **Only what the sources write, and every row each one writes**: the
/// pages and files fetched on 2026-11-14, of Microsoft's list for Windows,
/// Apple's for macOS, GNOME's window manager's defaults and KDE Plasma's
/// search launcher and window manager. A range a source writes once — a
/// digit from 0 to 9 — is a row per key. Such a chord is accepted with a
/// warning (`NOCTUA.md §12.8`), so a row missing here is a warning not given,
/// never a choice refused; and a row added is a fact copied from a source,
/// with the source beside it, never from memory.
pub static SYSTEM_SHORTCUTS: [SystemShortcut; 98] = [
    windows(&[Modifier::Alt], Key::Space),
    windows(&[Modifier::Alt], Key::Tab),
    windows(&[Modifier::Control, Modifier::Alt], Key::Tab),
    windows(&[Modifier::Control], Key::Space),
    windows(&[Modifier::Super], Key::Space),
    windows(&[Modifier::Shift, Modifier::Super], Key::Space),
    windows(&[Modifier::Control, Modifier::Super], Key::Space),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit0),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit1),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit2),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit3),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit4),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit5),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit6),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit7),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit8),
    windows(&[Modifier::Alt, Modifier::Super], Key::Digit9),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit0),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit1),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit2),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit3),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit4),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit5),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit6),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit7),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit8),
    windows(&[Modifier::Control, Modifier::Super], Key::Digit9),
    windows(&[Modifier::Super], Key::Digit0),
    windows(&[Modifier::Super], Key::Digit1),
    windows(&[Modifier::Super], Key::Digit2),
    windows(&[Modifier::Super], Key::Digit3),
    windows(&[Modifier::Super], Key::Digit4),
    windows(&[Modifier::Super], Key::Digit5),
    windows(&[Modifier::Super], Key::Digit6),
    windows(&[Modifier::Super], Key::Digit7),
    windows(&[Modifier::Super], Key::Digit8),
    windows(&[Modifier::Super], Key::Digit9),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit0),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit1),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit2),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit3),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit4),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit5),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit6),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit7),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit8),
    windows(&[Modifier::Shift, Modifier::Super], Key::Digit9),
    windows(&[Modifier::Alt, Modifier::Super], Key::ArrowUp),
    windows(&[Modifier::Alt, Modifier::Super], Key::ArrowDown),
    windows(&[Modifier::Alt, Modifier::Super], Key::KeyB),
    windows(&[Modifier::Alt, Modifier::Super], Key::KeyD),
    windows(&[Modifier::Alt, Modifier::Super], Key::KeyH),
    windows(&[Modifier::Alt, Modifier::Super], Key::KeyK),
    mac(&[Modifier::Super], Key::Space),
    mac(&[Modifier::Alt, Modifier::Super], Key::Space),
    mac(&[Modifier::Control, Modifier::Super], Key::Space),
    mac(&[Modifier::Control], Key::Space),
    mac(&[Modifier::Control, Modifier::Alt], Key::Space),
    mac(&[Modifier::Shift, Modifier::Super], Key::Space),
    gnome(&[Modifier::Alt], Key::Space),
    gnome(&[Modifier::Super], Key::Space),
    gnome(&[Modifier::Shift, Modifier::Super], Key::Space),
    gnome(&[Modifier::Alt], Key::Tab),
    gnome(&[Modifier::Super], Key::Tab),
    gnome(&[Modifier::Alt, Modifier::Shift], Key::Tab),
    gnome(&[Modifier::Shift, Modifier::Super], Key::Tab),
    gnome(&[Modifier::Alt], Key::Backquote),
    gnome(&[Modifier::Super], Key::Backquote),
    gnome(&[Modifier::Alt, Modifier::Super], Key::ArrowLeft),
    gnome(&[Modifier::Alt, Modifier::Super], Key::ArrowRight),
    gnome(&[Modifier::Control, Modifier::Alt], Key::ArrowLeft),
    gnome(&[Modifier::Control, Modifier::Alt], Key::ArrowRight),
    gnome(&[Modifier::Control, Modifier::Alt], Key::ArrowUp),
    gnome(&[Modifier::Control, Modifier::Alt], Key::ArrowDown),
    gnome(
        &[Modifier::Alt, Modifier::Shift, Modifier::Super],
        Key::ArrowLeft,
    ),
    gnome(
        &[Modifier::Alt, Modifier::Shift, Modifier::Super],
        Key::ArrowRight,
    ),
    gnome(
        &[Modifier::Control, Modifier::Alt, Modifier::Shift],
        Key::ArrowLeft,
    ),
    gnome(
        &[Modifier::Control, Modifier::Alt, Modifier::Shift],
        Key::ArrowRight,
    ),
    gnome(
        &[Modifier::Control, Modifier::Alt, Modifier::Shift],
        Key::ArrowUp,
    ),
    gnome(
        &[Modifier::Control, Modifier::Alt, Modifier::Shift],
        Key::ArrowDown,
    ),
    gnome(&[Modifier::Super], Key::ArrowUp),
    gnome(&[Modifier::Super], Key::ArrowDown),
    gnome(&[Modifier::Alt], Key::F2),
    gnome(&[Modifier::Alt], Key::F4),
    gnome(&[Modifier::Alt], Key::F5),
    gnome(&[Modifier::Alt], Key::F10),
    gnome(&[Modifier::Super], Key::KeyH),
    gnome(&[Modifier::Super], Key::Home),
    gnome(&[Modifier::Super], Key::End),
    gnome(&[Modifier::Super], Key::PageUp),
    gnome(&[Modifier::Super], Key::PageDown),
    kde(KRUNNER, &[Modifier::Alt], Key::Space),
    kde(KRUNNER, &[Modifier::Alt], Key::F2),
    kde(KWIN_TABBOX, &[Modifier::Alt], Key::Tab),
    kde(KWIN_TABBOX, &[Modifier::Super], Key::Tab),
    kde(KWIN_TABBOX, &[Modifier::Alt, Modifier::Shift], Key::Tab),
    kde(KWIN_TABBOX, &[Modifier::Shift, Modifier::Super], Key::Tab),
    kde(KWIN_USERACTIONS, &[Modifier::Alt], Key::F3),
];

#[cfg(test)]
mod tests {
    use super::{
        Binding, Chord, Claim, Key, KeyBoundTwice, Keyboard, Leader, LeaderPart, LeaderRule,
        Malformed, Modifier, Modifiers, SYSTEM_SHORTCUTS, Table, Verdict, Warning, apart, intra,
        settle, tables, warnings,
    };

    /// `held` and Space, as a Leader.
    fn leader(held: &[Modifier]) -> Leader {
        Leader::of(Chord::new(Modifiers::of(held), Key::Space)).expect("a chord holding a modifier")
    }

    /// `held` and Space as a Leader, with `chord` its chord modifier.
    fn part(held: &[Modifier], chord: Modifier) -> LeaderPart {
        LeaderPart {
            leader: leader(held),
            chord,
        }
    }

    /// The bindings each text is read as.
    fn bound(texts: &[&str]) -> Vec<Binding> {
        texts
            .iter()
            .map(|text| Binding::parse(text).expect("a binding"))
            .collect()
    }

    /// Every key is named as `KeyboardEvent.code` names it, and read back.
    ///
    /// **Both ways, over the whole vocabulary**: a row of the table out of
    /// the enum's order would name one key as another, and read back as that
    /// other. No modifier, lock key, numpad key or `Escape` is a key.
    #[test]
    fn every_key_is_named_as_keyboard_event_code_names_it_and_read_back() {
        for (at, key) in Key::ALL.into_iter().enumerate() {
            assert_eq!(key as usize, at, "{key:?} is out of the vocabulary's order");
            assert_eq!(Key::of_code(key.code()), Some(key), "{key:?}");
            assert_eq!(
                format!("{key:?}"),
                key.code(),
                "{key:?} is named another way"
            );
        }
        for code in [
            "Escape",
            "ShiftLeft",
            "ControlRight",
            "AltLeft",
            "MetaLeft",
            "CapsLock",
            "NumLock",
            "Numpad1",
            "NumpadEnter",
            "Fn",
            "IntlBackslash",
            "MediaPlayPause",
            "F13",
            "keyS",
            "KEYS",
            "S",
            "",
        ] {
            assert_eq!(
                Key::of_code(code),
                None,
                "`{code}` is not a key of the vocabulary"
            );
        }
    }

    /// A binding is one of the three forms, spelled one way, and nothing else.
    ///
    /// **Acceptance 7 of Stage 14**, at its root: the Emacs forms, aliases,
    /// lower case, a literal modifier, a Leader with no key or two, a doubled
    /// space and an invisible character are each refused.
    #[test]
    fn a_binding_is_one_of_the_three_forms_and_nothing_else() {
        for (text, table, key) in [
            ("Leader KeyS", Table::Menu, Key::KeyS),
            ("Leader+KeyQ", Table::Held, Key::KeyQ),
            ("Leader+Chord+KeyQ", Table::HeldWithChord, Key::KeyQ),
            ("Leader Digit1", Table::Menu, Key::Digit1),
            ("Leader+Equal", Table::Held, Key::Equal),
            (
                "Leader+Chord+ArrowLeft",
                Table::HeldWithChord,
                Key::ArrowLeft,
            ),
            ("Leader Space", Table::Menu, Key::Space),
        ] {
            let binding = Binding::parse(text).unwrap_or_else(|why| panic!("`{text}`: {why}"));
            assert_eq!(binding, Binding { table, key }, "`{text}`");
            assert_eq!(binding.written(), text);
        }
        for text in [
            "C-x C-s",
            "C-c j",
            "SPC b b",
            "M-S-w",
            "F2",
            "KeyS",
            "Leader s",
            "Leader S",
            "leader KeyS",
            "LEADER KeyS",
            "Leader keys",
            "Leader+Shift+KeyQ",
            "Leader+Control+KeyQ",
            "Leader+Chord+Shift+KeyQ",
            "Leader",
            "Leader ",
            "Leader+",
            "Leader+Chord",
            "Leader+Chord+",
            "Leader KeyW KeyM",
            "Leader  KeyS",
            " Leader KeyS",
            "Leader KeyS ",
            "Leader\tKeyS",
            "Leader Escape",
            "Leader ShiftLeft",
            "Leader Chord+KeyQ",
            "Leader+Chord KeyQ",
            "Leader\u{200b} KeyS",
            "Leader KeyS\u{2063}",
            "",
        ] {
            assert!(
                Binding::parse(text).is_err(),
                "`{text}` was read as {:?}",
                Binding::parse(text)
            );
        }
    }

    /// A refusal says what it read, escaped and cut short, and why.
    #[test]
    fn a_refusal_quotes_what_it_read_escaped_and_cut_short() {
        assert_eq!(
            Binding::parse("C-c j"),
            Err(Malformed::NotAForm("C-c j".to_owned()))
        );
        assert_eq!(
            Binding::parse("Leader+Shift+KeyQ"),
            Err(Malformed::NotAKey {
                written: "Leader+Shift+KeyQ".to_owned(),
                key: "Shift+KeyQ".to_owned(),
            })
        );
        let said = Binding::parse("Leader \u{1b}[2J")
            .expect_err("no key")
            .to_string();
        assert!(said.contains("\\u{1b}[2J"), "{said}");
        assert!(!said.contains('\u{1b}'), "{said}");
        let long = format!("Leader {}", "x".repeat(500));
        let said = Binding::parse(&long).expect_err("no key").to_string();
        assert!(said.len() < 300, "a refusal repeated {} bytes", said.len());
        assert!(said.contains('…'), "{said}");
    }

    /// A chord names each of its modifiers once, in one order, before one key.
    #[test]
    fn a_chord_names_its_modifiers_once_and_in_one_order() {
        for text in [
            "Space",
            "Alt+Space",
            "Control+Alt+Space",
            "Alt+Super+Space",
            "Control+Alt+Shift+Super+KeyQ",
            "Shift+Super+Digit0",
        ] {
            let chord = Chord::parse(text).unwrap_or_else(|why| panic!("`{text}`: {why}"));
            assert_eq!(chord.written(), text);
        }
        assert_eq!(
            Chord::parse("Control+Alt+Space"),
            Ok(Chord::new(
                Modifiers::of(&[Modifier::Control, Modifier::Alt]),
                Key::Space
            ))
        );
        for (text, why) in [
            ("Alt+Control+Space", "OutOfOrder"),
            ("Alt+Alt+Space", "OutOfOrder"),
            ("Super+Shift+Space", "OutOfOrder"),
            ("Ctrl+Alt+Space", "NotAModifier"),
            ("Meta+Space", "NotAModifier"),
            ("alt+Space", "NotAModifier"),
            ("+Space", "NotAModifier"),
            ("Alt++Space", "NotAModifier"),
            ("Alt+", "NotAKey"),
            ("Alt+Shift", "NotAKey"),
            ("Alt+space", "NotAKey"),
            ("", "NotAKey"),
            ("Alt Space", "NotAKey"),
        ] {
            let refused = Chord::parse(text).expect_err(text);
            assert!(
                format!("{refused:?}").starts_with(why),
                "`{text}` was refused as {refused:?}, not {why}"
            );
        }
    }

    /// Each keyboard labels a chord as its keys say.
    ///
    /// **The strings `§12.8`'s table writes**, and the three keys a Mac calls
    /// otherwise; Shift is Shift on every keyboard.
    #[test]
    fn each_keyboard_labels_a_chord_as_its_keys_say() {
        let staging = Chord::parse("Control+Alt+Space").expect("a chord");
        assert_eq!(
            Keyboard::ALL.map(|keyboard| staging.written_for(keyboard)),
            ["Ctrl+Alt+Space", "Ctrl+Alt+Space", "Control+Option+Space"]
        );
        let development = Chord::parse("Alt+Super+Space").expect("a chord");
        assert_eq!(
            Keyboard::ALL.map(|keyboard| development.written_for(keyboard)),
            ["Alt+Super+Space", "Alt+Win+Space", "Option+Command+Space"]
        );
        let shifted = Chord::parse("Shift+Super+KeyS").expect("a chord");
        assert_eq!(
            Keyboard::ALL.map(|keyboard| shifted.written_for(keyboard)),
            ["Shift+Super+S", "Shift+Win+S", "Shift+Command+S"]
        );
        for (key, everywhere, mac) in [
            (Key::Enter, "Enter", "Return"),
            (Key::Backspace, "Backspace", "Delete"),
            (Key::Delete, "Delete", "Forward Delete"),
            (Key::Equal, "=", "="),
            (Key::ArrowLeft, "Left", "Left"),
            (Key::Digit1, "1", "1"),
        ] {
            assert_eq!(key.written_for(Keyboard::Linux), everywhere, "{key:?}");
            assert_eq!(key.written_for(Keyboard::Windows), everywhere, "{key:?}");
            assert_eq!(key.written_for(Keyboard::Mac), mac, "{key:?}");
        }
        let under = part(&[Modifier::Alt], Modifier::Shift);
        assert_eq!(
            bound(&["Leader KeyS", "Leader+KeyQ", "Leader+Chord+KeyQ"])
                .into_iter()
                .map(|binding| binding.written_for(under, Keyboard::Mac))
                .collect::<Vec<_>>(),
            ["Option+Space S", "Option+Space+Q", "Option+Shift+Space+Q"]
        );
    }

    /// A Leader holds at least one modifier.
    #[test]
    fn a_leader_with_no_modifier_is_refused() {
        assert_eq!(
            Leader::of(Chord::new(Modifiers::NONE, Key::Space)),
            Err(LeaderRule::WithoutAModifier)
        );
        assert_eq!(
            Leader::of(Chord::new(Modifiers::of(&[Modifier::Shift]), Key::Space))
                .map(Leader::written),
            Ok("Shift+Space".to_owned())
        );
    }

    /// A Leader that holds its own chord modifier is refused, rule 1.
    #[test]
    fn a_leader_that_holds_its_chord_modifier_is_refused() {
        assert_eq!(
            intra(part(&[Modifier::Alt, Modifier::Shift], Modifier::Shift)),
            Err(LeaderRule::HoldsTheChordModifier)
        );
        assert_eq!(
            intra(part(&[Modifier::Shift], Modifier::Shift)),
            Err(LeaderRule::HoldsTheChordModifier)
        );
        assert_eq!(intra(part(&[Modifier::Alt], Modifier::Shift)), Ok(()));
        assert_eq!(intra(part(&[Modifier::Shift], Modifier::Control)), Ok(()));
    }

    /// A Leader held with its chord modifier is never another's Leader, rule 2.
    ///
    /// **Whichever of the two is asked about first**: the pair is judged both
    /// ways round.
    #[test]
    fn a_leader_held_with_its_chord_that_is_another_channels_leader_is_refused() {
        let staging = part(&[Modifier::Control, Modifier::Alt], Modifier::Shift);
        let production = part(&[Modifier::Alt], Modifier::Control);
        assert_eq!(
            apart(production, staging),
            Err(LeaderRule::HeldWithTheChordIsAnothers)
        );
        assert_eq!(
            apart(staging, production),
            Err(LeaderRule::HeldWithTheChordIsAnothers)
        );
        let shifted = part(&[Modifier::Alt], Modifier::Shift);
        assert_eq!(apart(shifted, staging), Ok(()));
    }

    /// No two channels share a Leader, rule 3, whatever their chord modifiers.
    #[test]
    fn two_channels_never_share_a_leader() {
        assert_eq!(
            apart(
                part(&[Modifier::Alt], Modifier::Shift),
                part(&[Modifier::Alt], Modifier::Control)
            ),
            Err(LeaderRule::SharedWithAnother)
        );
        assert_eq!(
            apart(
                part(&[Modifier::Alt], Modifier::Shift),
                part(&[Modifier::Alt, Modifier::Super], Modifier::Shift)
            ),
            Ok(())
        );
    }

    /// Two development checkouts on their defaults are not a collision; the
    /// moment one configures the same Leader, it is.
    #[test]
    fn two_development_checkouts_on_their_defaults_are_not_a_collision() {
        let development = part(&[Modifier::Alt, Modifier::Super], Modifier::Shift);
        let on_defaults = [1, 2].map(|rank| Claim {
            rank,
            default: development,
            configured: None,
        });
        for settled in settle(&on_defaults) {
            assert_eq!(settled.verdict, Verdict::Default);
            assert_eq!(settled.part, development);
        }
        let mut configured = on_defaults;
        configured[1].configured = Some(development);
        let settled = settle(&configured);
        assert_eq!(settled[0].verdict, Verdict::Default);
        assert_eq!(
            settled[1].verdict,
            Verdict::Refused {
                rule: LeaderRule::SharedWithAnother,
                by: Some(1)
            }
        );
    }

    /// A part is settled against the parts before it and the defaults after
    /// it, and what it is refused for names the rule and the claim.
    #[test]
    fn a_configured_leader_is_judged_against_those_before_it_and_the_defaults_after() {
        let production = part(&[Modifier::Alt], Modifier::Shift);
        let staging = part(&[Modifier::Control, Modifier::Alt], Modifier::Shift);
        let development = part(&[Modifier::Alt, Modifier::Super], Modifier::Shift);
        let free = part(&[Modifier::Control, Modifier::Super], Modifier::Shift);
        let claims = [
            Claim {
                rank: 0,
                default: production,
                configured: Some(development),
            },
            Claim {
                rank: 1,
                default: staging,
                configured: Some(free),
            },
            Claim {
                rank: 2,
                default: development,
                configured: Some(free),
            },
        ];
        let settled = settle(&claims);
        assert_eq!(
            settled[0].verdict,
            Verdict::Refused {
                rule: LeaderRule::SharedWithAnother,
                by: Some(2)
            },
            "production took development's default"
        );
        assert_eq!(settled[0].part, production);
        assert_eq!(settled[1].verdict, Verdict::Accepted);
        assert_eq!(settled[1].part, free);
        assert_eq!(
            settled[2].verdict,
            Verdict::Refused {
                rule: LeaderRule::SharedWithAnother,
                by: Some(1)
            },
            "development took what staging had settled on"
        );
        assert_eq!(settled[2].part, development);
        let holding = settle(&[Claim {
            rank: 0,
            default: production,
            configured: Some(part(&[Modifier::Alt, Modifier::Shift], Modifier::Shift)),
        }]);
        assert_eq!(
            holding[0].verdict,
            Verdict::Refused {
                rule: LeaderRule::HoldsTheChordModifier,
                by: None
            }
        );
    }

    /// Two commands on one key of one table are refused, rule 4, and one key
    /// in two tables is two bindings.
    #[test]
    fn two_commands_on_one_key_of_one_table_are_refused_and_one_key_in_two_tables_is_not() {
        let defaults = [
            ("page.close", bound(&["Leader KeyQ", "Leader+KeyQ"])),
            ("page.quit", bound(&["Leader+Chord+KeyQ"])),
        ];
        let made = tables(&defaults, &[]).expect("one key in three tables");
        assert_eq!(made.len(), 3);
        assert_eq!(
            made.command(Binding {
                table: Table::HeldWithChord,
                key: Key::KeyQ
            }),
            Some("page.quit")
        );
        let clashing = [
            ("page.close", bound(&["Leader+KeyQ"])),
            ("page.quit", bound(&["Leader+KeyQ"])),
            ("page.again", bound(&["Leader KeyA", "Leader KeyA"])),
        ];
        assert_eq!(
            tables(&clashing, &[]),
            Err(vec![
                KeyBoundTwice {
                    binding: Binding {
                        table: Table::Held,
                        key: Key::KeyQ
                    },
                    first: "page.close".to_owned(),
                    second: "page.quit".to_owned(),
                },
                KeyBoundTwice {
                    binding: Binding {
                        table: Table::Menu,
                        key: Key::KeyA
                    },
                    first: "page.again".to_owned(),
                    second: "page.again".to_owned(),
                },
            ])
        );
    }

    /// A command a person chose keys for is bound to those in place of its
    /// defaults, an empty list unbinds it, and the defaults it no longer
    /// holds are free for another command.
    #[test]
    fn a_chosen_list_replaces_a_commands_defaults_and_an_empty_one_unbinds_it() {
        let defaults = [
            ("kernel.undo", bound(&["Leader+KeyZ"])),
            ("kernel.redo", bound(&["Leader+Chord+KeyZ"])),
        ];
        let chosen = [
            ("kernel.redo", Vec::new()),
            ("kernel.undo", bound(&["Leader+Chord+KeyZ"])),
            ("page.close", bound(&["Leader+KeyZ"])),
        ];
        let made = tables(&defaults, &chosen).expect("nothing is bound twice");
        assert_eq!(
            made.iter().collect::<Vec<_>>(),
            [
                (
                    Binding {
                        table: Table::Held,
                        key: Key::KeyZ
                    },
                    "page.close"
                ),
                (
                    Binding {
                        table: Table::HeldWithChord,
                        key: Key::KeyZ
                    },
                    "kernel.undo"
                ),
            ]
        );
        assert!(tables(&defaults, &[("kernel.undo", Vec::new())]).is_ok());
        assert!(
            tables(&defaults, &[("page.close", bound(&["Leader+KeyZ"]))]).is_err(),
            "a chosen binding met a default nobody replaced"
        );
    }

    /// A binding the system uses is kept and named in a warning, one per
    /// system that keeps its chord.
    #[test]
    fn a_binding_the_system_uses_is_kept_and_named_in_a_warning() {
        let production = part(&[Modifier::Alt], Modifier::Shift);
        let made = tables(&[("window.next", bound(&["Leader+Tab"]))], &[]).expect("one binding");
        let found = warnings(production, &made);
        let named: Vec<(Option<String>, &str, String)> = found
            .iter()
            .map(|warning| match warning {
                Warning::TheSystemUses { binding, shortcut } => (
                    binding.map(Binding::written),
                    shortcut.system,
                    shortcut.chord.written(),
                ),
                Warning::Unpressable { binding } => {
                    (Some(binding.written()), "unpressable", String::new())
                }
            })
            .collect();
        assert_eq!(
            named,
            [
                (None, "Windows", "Alt+Space".to_owned()),
                (None, "GNOME", "Alt+Space".to_owned()),
                (None, "KDE Plasma", "Alt+Space".to_owned()),
                (
                    Some("Leader+Tab".to_owned()),
                    "Windows",
                    "Alt+Tab".to_owned()
                ),
                (Some("Leader+Tab".to_owned()), "GNOME", "Alt+Tab".to_owned()),
                (
                    Some("Leader+Tab".to_owned()),
                    "KDE Plasma",
                    "Alt+Tab".to_owned()
                ),
            ]
        );
        assert_eq!(
            made.command(Binding {
                table: Table::Held,
                key: Key::Tab
            }),
            Some("window.next"),
            "a warning took the binding away"
        );
        let chorded =
            tables(&[("window.back", bound(&["Leader+Chord+Tab"]))], &[]).expect("one binding");
        assert!(
            warnings(production, &chorded).iter().any(|warning| matches!(
                warning,
                Warning::TheSystemUses { binding: Some(_), shortcut } if shortcut.chord.written() == "Alt+Shift+Tab"
            )),
            "the chord modifier was not pressed with the key"
        );
        let menu = tables(&[("window.next", bound(&["Leader Tab"]))], &[]).expect("one binding");
        assert!(
            warnings(production, &menu)
                .iter()
                .all(|warning| matches!(warning, Warning::TheSystemUses { binding: None, .. })),
            "a key in the leader menu is pressed inside the interface"
        );
    }

    /// A held key that is the Leader's own is a warning, in both held tables.
    #[test]
    fn a_held_key_that_is_the_leaders_own_is_a_warning() {
        let under = part(&[Modifier::Control, Modifier::Super], Modifier::Shift);
        let made = tables(
            &[(
                "page.close",
                bound(&[
                    "Leader Space",
                    "Leader+Space",
                    "Leader+Chord+Space",
                    "Leader+KeyQ",
                ]),
            )],
            &[],
        )
        .expect("one key in three tables");
        let unpressable: Vec<String> = warnings(under, &made)
            .iter()
            .filter_map(|warning| match warning {
                Warning::Unpressable { binding } => Some(binding.written()),
                Warning::TheSystemUses { .. } => None,
            })
            .collect();
        assert_eq!(unpressable, ["Leader+Space", "Leader+Chord+Space"]);
    }

    /// The system shortcuts are the rows their sources write, each with the
    /// source it was copied from.
    ///
    /// **Written out**, because the sources are pages and files outside this
    /// repository that no test may read: a row added, taken away or moved to
    /// another system is an edit here as well, which is where a reader checks
    /// it against the source the row names.
    #[test]
    fn the_system_shortcuts_are_the_rows_their_sources_write() {
        let written = |system: &str, source: &str| -> String {
            SYSTEM_SHORTCUTS
                .iter()
                .filter(|row| row.system == system && row.source.ends_with(source))
                .map(|row| row.chord.written())
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(
            written(
                "Windows",
                "keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec"
            ),
            "Alt+Space Alt+Tab Control+Alt+Tab Control+Space Super+Space Shift+Super+Space \
             Control+Super+Space Alt+Super+Digit0 Alt+Super+Digit1 Alt+Super+Digit2 \
             Alt+Super+Digit3 Alt+Super+Digit4 Alt+Super+Digit5 Alt+Super+Digit6 \
             Alt+Super+Digit7 Alt+Super+Digit8 Alt+Super+Digit9 Control+Super+Digit0 \
             Control+Super+Digit1 Control+Super+Digit2 Control+Super+Digit3 \
             Control+Super+Digit4 Control+Super+Digit5 Control+Super+Digit6 \
             Control+Super+Digit7 Control+Super+Digit8 Control+Super+Digit9 Super+Digit0 \
             Super+Digit1 Super+Digit2 Super+Digit3 Super+Digit4 Super+Digit5 Super+Digit6 \
             Super+Digit7 Super+Digit8 Super+Digit9 Shift+Super+Digit0 Shift+Super+Digit1 \
             Shift+Super+Digit2 Shift+Super+Digit3 Shift+Super+Digit4 Shift+Super+Digit5 \
             Shift+Super+Digit6 Shift+Super+Digit7 Shift+Super+Digit8 Shift+Super+Digit9 \
             Alt+Super+ArrowUp Alt+Super+ArrowDown Alt+Super+KeyB Alt+Super+KeyD \
             Alt+Super+KeyH Alt+Super+KeyK"
        );
        assert_eq!(
            written("macOS", "support.apple.com/en-us/102650"),
            "Super+Space Alt+Super+Space Control+Super+Space Control+Space Control+Alt+Space \
             Shift+Super+Space"
        );
        assert_eq!(
            written("GNOME", "org.gnome.desktop.wm.keybindings.gschema.xml.in"),
            "Alt+Space Super+Space Shift+Super+Space Alt+Tab Super+Tab Alt+Shift+Tab \
             Shift+Super+Tab Alt+Backquote Super+Backquote Alt+Super+ArrowLeft \
             Alt+Super+ArrowRight Control+Alt+ArrowLeft Control+Alt+ArrowRight \
             Control+Alt+ArrowUp Control+Alt+ArrowDown Alt+Shift+Super+ArrowLeft \
             Alt+Shift+Super+ArrowRight Control+Alt+Shift+ArrowLeft \
             Control+Alt+Shift+ArrowRight Control+Alt+Shift+ArrowUp \
             Control+Alt+Shift+ArrowDown Super+ArrowUp Super+ArrowDown Alt+F2 Alt+F4 Alt+F5 \
             Alt+F10 Super+KeyH Super+Home Super+End Super+PageUp Super+PageDown"
        );
        assert_eq!(
            written("KDE Plasma", "krunner/org.kde.krunner.desktop.cmake"),
            "Alt+Space Alt+F2"
        );
        assert_eq!(
            written("KDE Plasma", "src/tabbox/tabbox.cpp"),
            "Alt+Tab Super+Tab Alt+Shift+Tab Shift+Super+Tab"
        );
        assert_eq!(written("KDE Plasma", "src/useractions.cpp"), "Alt+F3");
        for row in &SYSTEM_SHORTCUTS {
            assert!(row.source.starts_with("https://"), "{row:?}");
            let keyboard = match row.system {
                "Windows" => Keyboard::Windows,
                "macOS" => Keyboard::Mac,
                "GNOME" | "KDE Plasma" => Keyboard::Linux,
                other => panic!("`{other}` is no system a source here names"),
            };
            assert_eq!(row.keyboard, keyboard, "{row:?}");
            assert_eq!(Chord::parse(&row.chord.written()), Ok(row.chord), "{row:?}");
        }
    }
}

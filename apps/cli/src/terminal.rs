//! The thin shell around the picker: the terminal put in raw mode on the
//! alternate screen, keys read and the picker's lines drawn — through
//! `crossterm` — and **the terminal left as it was found however the picker
//! ends**: on a choice, on Esc, on an error or on a panic (`NOCTUA.md §17.1`).
//!
//! **One guard leaves it**, made before the terminal is entered and dropped on
//! every way out of [`picked`], an unwinding panic's included: a terminal
//! entered halfway — raw mode on and the alternate screen refused — is left
//! too. What the picker decides is [`crate::picker`]'s, a pure model; nothing
//! here decides anything but how a key is read.
//!
//! # What this does not answer
//!
//! A process ended by a signal it cannot catch, which leaves raw mode on; the
//! terminal's own `reset` restores it.

use std::io::Write as _;

use crate::picker::{Key, Picker, Step};

/// A terminal the picker runs in.
pub trait Terminal {
    /// Raw mode on, the alternate screen entered and the cursor hidden.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] as the terminal answered it.
    fn enter(&mut self) -> std::io::Result<()>;

    /// The terminal as it was before [`Terminal::enter`]: the cursor shown,
    /// the alternate screen left and raw mode off.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] as the terminal answered it.
    fn leave(&mut self) -> std::io::Result<()>;

    /// Its width and its height, in characters and lines.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] as the terminal answered it.
    fn size(&mut self) -> std::io::Result<(usize, usize)>;

    /// The next key pressed.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] as the terminal answered it.
    fn key(&mut self) -> std::io::Result<Key>;

    /// Draws `lines` from the top of the screen, and nothing else.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] as the terminal answered it.
    fn draw(&mut self, lines: &[String]) -> std::io::Result<()>;
}

/// The terminal, left when this is dropped.
struct Entered<'a, T: Terminal>(&'a mut T);

impl<T: Terminal> Drop for Entered<'_, T> {
    fn drop(&mut self) {
        // Nothing is left to tell of a terminal that would not be left: the
        // run ends either way, and what it ends with says why.
        drop(self.0.leave());
    }
}

/// What `picker` comes to in `terminal`: the index of the entry chosen, or
/// nothing when it was left; the terminal left as it was found either way.
///
/// # Errors
///
/// What the terminal answered when it did not answer as one: raw mode, the
/// screen, a key or a line.
pub fn picked<T: Terminal>(
    terminal: &mut T,
    picker: &mut Picker,
) -> std::io::Result<Option<usize>> {
    let entered = Entered(terminal);
    entered.0.enter()?;
    loop {
        let (width, height) = entered.0.size()?;
        entered.0.draw(&picker.lines(width, height))?;
        match picker.key(entered.0.key()?) {
            Step::Stay => {}
            Step::Chosen(index) => return Ok(Some(index)),
            Step::Left => return Ok(None),
        }
    }
}

/// The terminal at this process's standard input and output, reached
/// through `crossterm`.
#[derive(Debug)]
pub struct Crossterm(std::io::Stdout);

impl Crossterm {
    /// The terminal at this process's standard output.
    #[must_use]
    #[expect(
        clippy::disallowed_methods,
        reason = "the picker draws on the terminal it was opened at, which the CLI checked is \
                  its standard output, and nothing else is printed through it"
    )]
    pub fn new() -> Self {
        Self(std::io::stdout())
    }
}

impl Default for Crossterm {
    fn default() -> Self {
        Self::new()
    }
}

impl Terminal for Crossterm {
    fn enter(&mut self) -> std::io::Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            self.0,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::cursor::Hide
        )
    }

    fn leave(&mut self) -> std::io::Result<()> {
        let shown = crossterm::execute!(
            self.0,
            crossterm::cursor::Show,
            crossterm::terminal::LeaveAlternateScreen
        );
        // **Raw mode off whatever the screen answered**, so a terminal that
        // would not leave the alternate screen still reads lines again.
        let cooked = crossterm::terminal::disable_raw_mode();
        shown.and(cooked)
    }

    fn size(&mut self) -> std::io::Result<(usize, usize)> {
        let (width, height) = crossterm::terminal::size()?;
        Ok((usize::from(width), usize::from(height)))
    }

    fn key(&mut self) -> std::io::Result<Key> {
        loop {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(event) => {
                    if let Some(key) = read_key(event) {
                        return Ok(key);
                    }
                }
                crossterm::event::Event::Resize(..) => return Ok(Key::Other),
                _ => {}
            }
        }
    }

    fn draw(&mut self, lines: &[String]) -> std::io::Result<()> {
        crossterm::queue!(
            self.0,
            crossterm::cursor::MoveTo(0, 0),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
        )?;
        for (at, line) in lines.iter().enumerate() {
            if at > 0 {
                // Raw mode moves to the next line, not to its start.
                self.0.write_all(b"\r\n")?;
            }
            self.0.write_all(line.as_bytes())?;
        }
        self.0.flush()
    }
}

/// The key a key event is to the picker, or nothing for one it does not
/// read: a release, which Windows reports beside each press.
fn read_key(event: crossterm::event::KeyEvent) -> Option<Key> {
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

    if event.kind == KeyEventKind::Release {
        return None;
    }
    let control = event.modifiers.contains(KeyModifiers::CONTROL);
    Some(match event.code {
        KeyCode::Char('c') if control => Key::Interrupt,
        KeyCode::Char('p') if control => Key::Up,
        KeyCode::Char('n') if control => Key::Down,
        KeyCode::Char(_) if control || event.modifiers.contains(KeyModifiers::ALT) => Key::Other,
        KeyCode::Char(character) => Key::Character(character),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Escape,
        _ => Key::Other,
    })
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::{Terminal, picked, read_key};
    use crate::picker::{Entry, Key, Picker};

    /// A terminal that answers as a script says and writes down what it was
    /// asked.
    struct Scripted {
        /// The keys it hands out, in order; an error past the last.
        keys: Vec<Key>,
        /// Whether entering fails, after raw mode is on.
        enter_fails: bool,
        /// Whether drawing panics.
        draw_panics: bool,
        /// What it was asked, in order.
        seen: Vec<&'static str>,
    }

    impl Scripted {
        fn new(keys: &[Key]) -> Self {
            Self {
                keys: keys.iter().rev().copied().collect(),
                enter_fails: false,
                draw_panics: false,
                seen: Vec::new(),
            }
        }
    }

    impl Terminal for Scripted {
        fn enter(&mut self) -> std::io::Result<()> {
            self.seen.push("enter");
            if self.enter_fails {
                return Err(std::io::Error::other("the alternate screen was refused"));
            }
            Ok(())
        }

        fn leave(&mut self) -> std::io::Result<()> {
            self.seen.push("leave");
            Ok(())
        }

        fn size(&mut self) -> std::io::Result<(usize, usize)> {
            Ok((40, 10))
        }

        fn key(&mut self) -> std::io::Result<Key> {
            self.keys
                .pop()
                .ok_or_else(|| std::io::Error::other("the terminal went away"))
        }

        fn draw(&mut self, lines: &[String]) -> std::io::Result<()> {
            assert_ne!(lines.len(), 0, "nothing was drawn");
            assert!(!self.draw_panics, "a drawing that panics");
            self.seen.push("draw");
            Ok(())
        }
    }

    fn picker() -> Picker {
        Picker::new(vec![
            Entry {
                digest: "0123abcd".to_owned(),
                directory: Some("/a".to_owned()),
                note: String::new(),
                chosen: false,
            },
            Entry {
                digest: "89efcd45".to_owned(),
                directory: Some("/b".to_owned()),
                note: String::new(),
                chosen: true,
            },
        ])
    }

    /// The terminal is left as it was found on every way out: a choice, Esc,
    /// a key that never came, an entering that failed halfway and a panic.
    #[test]
    fn the_terminal_is_left_on_every_way_out_an_early_return_and_a_panic_included() {
        let mut chose = Scripted::new(&[Key::Down, Key::Enter]);
        assert_eq!(picked(&mut chose, &mut picker()).expect("chosen"), Some(1));
        assert_eq!(chose.seen, ["enter", "draw", "draw", "leave"]);

        let mut left = Scripted::new(&[Key::Character('b'), Key::Escape]);
        assert_eq!(picked(&mut left, &mut picker()).expect("left"), None);
        assert_eq!(left.seen.last(), Some(&"leave"));

        let mut unanswered = Scripted::new(&[Key::Down]);
        assert!(picked(&mut unanswered, &mut picker()).is_err());
        assert_eq!(
            unanswered.seen,
            ["enter", "draw", "draw", "leave"],
            "an early return left the terminal raw"
        );

        let mut halfway = Scripted::new(&[]);
        halfway.enter_fails = true;
        assert!(picked(&mut halfway, &mut picker()).is_err());
        assert_eq!(
            halfway.seen,
            ["enter", "leave"],
            "a half-entered terminal was kept"
        );

        let mut panicking = Scripted::new(&[Key::Enter]);
        panicking.draw_panics = true;
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            drop(picked(&mut panicking, &mut picker()));
        }));
        assert!(unwound.is_err(), "the drawing did not panic");
        assert_eq!(
            panicking.seen,
            ["enter", "leave"],
            "a panic left the terminal raw"
        );
    }

    /// A key event is read as the picker's key.
    ///
    /// Ctrl-C, Ctrl-P and Ctrl-N as they arrive in raw mode, a release as
    /// nothing, and another chord as no character typed.
    #[test]
    fn a_key_event_is_read_as_the_pickers_key() {
        let press = |code, modifiers| KeyEvent::new(code, modifiers);
        let none = KeyModifiers::NONE;
        let control = KeyModifiers::CONTROL;
        for (event, key) in [
            (press(KeyCode::Char('c'), control), Key::Interrupt),
            (press(KeyCode::Char('p'), control), Key::Up),
            (press(KeyCode::Char('n'), control), Key::Down),
            (press(KeyCode::Char('x'), control), Key::Other),
            (press(KeyCode::Char('x'), KeyModifiers::ALT), Key::Other),
            (press(KeyCode::Char('é'), none), Key::Character('é')),
            (
                press(KeyCode::Char('A'), KeyModifiers::SHIFT),
                Key::Character('A'),
            ),
            (press(KeyCode::Backspace, none), Key::Backspace),
            (press(KeyCode::Up, none), Key::Up),
            (press(KeyCode::Down, none), Key::Down),
            (press(KeyCode::Enter, none), Key::Enter),
            (press(KeyCode::Esc, none), Key::Escape),
            (press(KeyCode::Tab, none), Key::Other),
        ] {
            assert_eq!(read_key(event), Some(key), "{event:?}");
        }
        let mut released = press(KeyCode::Enter, none);
        released.kind = KeyEventKind::Release;
        assert_eq!(read_key(released), None);
    }
}

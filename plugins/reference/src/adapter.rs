//! The world's side of the reference plugin: [`crate::behaviour`] in the
//! world's types, the store read its activation comes from, every import the
//! probe calls, and the one `export!`.
//!
//! Compiled for `wasm32` only — see the crate root for why a host build cannot
//! carry it — so the host target neither lints nor tests it. `cargo xtask check`
//! lints and documents this crate for `wasm32` as well, which is the only run
//! that reads this file, and the kernel's own suite drives the component it
//! becomes: `crates/nc-kernel/tests/capabilities.rs` holds what the probe
//! reaches through [`World`] to every function of the world.
//!
//! **The condition is this file's own, as an inner attribute.** Written on the
//! `mod adapter;` line it reads, to the gate that refuses a hidden test, as
//! every test of this file hidden behind a condition; this file holds none,
//! and the inner attribute is the form that gate reads as about this file
//! alone.
#![cfg(target_arch = "wasm32")]

use nc_guest::generated::exports::noctua::kernel::guest::Guest;
use nc_guest::generated::noctua::kernel::commands::{Descriptor, Manifest, Reversibility};
use nc_guest::generated::noctua::kernel::types::Error;
use nc_guest::generated::noctua::kernel::{
    blobs, commands, context, events, log, net, store, timers, ui,
};

use crate::behaviour::{self, Outcome, Refused};

/// The plugin, as the world's `guest` interface sees it.
struct Plugin;

impl Guest for Plugin {
    /// Declares the table as the store's [`behaviour::ACTIVATION`] says.
    ///
    /// Or as written when the store holds nothing there or is not granted, and
    /// subscribes to what it says.
    fn activate() -> Result<Manifest, Error> {
        let activation = activation()?;
        assert!(
            !activation.traps(),
            "the activation the store holds asks `activate` to trap"
        );
        Ok(Manifest {
            commands: behaviour::commands_for(&activation)
                .into_iter()
                .map(descriptor)
                .collect(),
            subscriptions: activation.subscribe,
        })
    }

    fn deactivate() {}

    fn execute(name: String, args: String) -> Result<String, Error> {
        let namespace = name.split('.').next().unwrap_or(&name).to_owned();
        behaviour::execute(&name, &args, &mut World::under(namespace)).map_err(error)
    }

    /// Hears an event as the plugin of the namespace the store's activation
    /// names, as `activate` read it.
    fn on_event(topic: String, data: String) {
        let namespace = activation().map_or_else(
            |_| behaviour::NAMESPACE.to_owned(),
            |activation| activation.namespace,
        );
        behaviour::on_event(
            &namespace.clone(),
            &topic,
            &data,
            &mut World::under(namespace),
        );
    }

    /// Is woken by a timer as the plugin of the namespace the store's
    /// activation names.
    fn on_timer(tag: String) {
        let namespace = activation().map_or_else(
            |_| behaviour::NAMESPACE.to_owned(),
            |activation| activation.namespace,
        );
        behaviour::on_timer(&namespace.clone(), &tag, &mut World::under(namespace));
    }
}

/// How the plugin activates, as its store's [`behaviour::ACTIVATION`] says,
/// or as written when the store holds nothing there or is not granted.
fn activation() -> Result<behaviour::Activation, Error> {
    match store::get(behaviour::ACTIVATION) {
        Ok(Some(entry)) => behaviour::Activation::read(&entry.value).map_err(error),
        Ok(None) | Err(Error::Denied(_)) => Ok(behaviour::Activation::default()),
        Err(other) => Err(other),
    }
}

/// The world's imports, called with the probe's own arguments.
///
/// And the namespace the call runs under, which the probe publishes in, and
/// the units the command running now holds: each dropped, and so let go, when
/// the command returns.
#[derive(Default)]
struct World {
    namespace: String,
    units: Vec<Option<commands::Unit>>,
}

impl World {
    /// The world of a call under `namespace`, holding no unit yet.
    fn under(namespace: String) -> Self {
        Self {
            namespace,
            units: Vec::new(),
        }
    }

    /// The unit held at `unit`.
    fn unit(&self, unit: usize) -> Result<&commands::Unit, Refused> {
        self.units
            .get(unit)
            .and_then(Option::as_ref)
            .ok_or_else(|| Refused::Invalid(format!("no unit is held at {unit}")))
    }
}

/// The blob every `blobs` call of the probe names: the hash of nothing in
/// particular, well formed.
const A_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// A note's arguments, for the compensation the probe registers.
const A_NOTE: &str = r#"{"note":"photo"}"#;

/// The command the probe dispatches: one of the kernel's own, which runs
/// beside any plugin — a command of this plugin's would be a dispatch into
/// itself while it runs, which the kernel refuses.
const DISPATCHED: &str = "journal.verify";

impl behaviour::Imports for World {
    fn call(&mut self, function: &str) -> Outcome {
        match function {
            "log#write" => {
                log::write(log::Level::Info, "the probe reached the log");
                Outcome::Answered
            }
            "context#principal" => outcome(context::principal()),
            "context#actor-chain" => outcome(context::actor_chain()),
            "context#confirmed-by-human" => outcome(context::confirmed_by_human()),
            "store#get" => outcome(store::get("probe")),
            "store#put" => outcome(store::put("probe", "{}", None)),
            "store#delete" => outcome(store::delete("probe", None)),
            "store#list" => outcome(store::list("", 1, None)),
            "events#publish" => {
                outcome(events::publish(&format!("{}.probed", self.namespace), "{}"))
            }
            other => Self::call_further(other),
        }
    }

    fn get(&mut self, key: &str) -> Result<Option<behaviour::Stored>, Refused> {
        store::get(key)
            .map(|held| {
                held.map(|entry| behaviour::Stored {
                    value: entry.value,
                    rev: entry.rev,
                })
            })
            .map_err(refused)
    }

    fn put(&mut self, key: &str, value: &str, expected: Option<&str>) -> Result<String, Refused> {
        store::put(key, value, expected).map_err(refused)
    }

    fn delete(&mut self, key: &str, expected: Option<&str>) -> Result<(), Refused> {
        store::delete(key, expected).map_err(refused)
    }

    fn dispatch(&mut self, name: &str, args: &str) -> Result<String, Refused> {
        commands::dispatch(name, args).map_err(refused)
    }

    fn locate(&mut self, hash: &str) -> Result<String, Refused> {
        let word = match blobs::locate(hash).map_err(refused)? {
            blobs::Availability::Local => "local",
            blobs::Availability::Remote => "remote",
            blobs::Availability::Unavailable => "unavailable",
        };
        Ok(word.to_owned())
    }

    fn put_blob(&mut self, piece: &[u8], times: u32) -> Result<String, Refused> {
        let writer = blobs::create().map_err(refused)?;
        for _ in 0..times {
            writer.write(piece).map_err(refused)?;
        }
        writer.finish().map_err(refused)
    }

    fn get_blob(&mut self, hash: &str, max: u32) -> Result<behaviour::Blob, Refused> {
        let reader = blobs::open(hash).map_err(refused)?;
        let size = reader.size();
        let (mut chunks, mut bytes) = (Vec::new(), Vec::new());
        loop {
            let chunk = reader.read(max).map_err(refused)?;
            if chunk.is_empty() {
                break;
            }
            chunks.push(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
            bytes.extend_from_slice(&chunk);
        }
        Ok(behaviour::Blob {
            size,
            chunks,
            bytes,
        })
    }

    fn begin(&mut self, label: &str) -> Result<usize, Refused> {
        let unit = commands::begin(label).map_err(refused)?;
        self.units.push(Some(unit));
        Ok(self.units.len() - 1)
    }

    fn compensate_with(&mut self, unit: usize, name: &str, args: &str) -> Result<(), Refused> {
        self.unit(unit)?
            .compensate_with(name, args)
            .map_err(refused)
    }

    fn commit(&mut self, unit: usize) -> Result<(), Refused> {
        self.unit(unit)?.commit().map_err(refused)
    }

    fn abort(&mut self, unit: usize) {
        if let Some(held) = self.units.get_mut(unit).and_then(Option::take) {
            held.abort();
        }
    }

    fn release(&mut self, unit: usize) {
        if let Some(slot) = self.units.get_mut(unit) {
            *slot = None;
        }
    }

    fn log(&mut self, level: &str, text: &str) {
        let level = match level {
            "trace" => log::Level::Trace,
            "debug" => log::Level::Debug,
            "warn" => log::Level::Warn,
            "error" => log::Level::Error,
            _ => log::Level::Info,
        };
        log::write(level, text);
    }

    fn publish(&mut self, topic: &str, data: &str) -> Result<(), Refused> {
        events::publish(topic, data).map_err(refused)
    }

    fn set_timer(&mut self, ms: u64, every: bool, tag: &str) -> Result<u64, Refused> {
        if every {
            timers::every(ms, tag)
        } else {
            timers::after(ms, tag)
        }
        .map_err(refused)
    }

    fn cancel(&mut self, timer: u64) -> Result<(), Refused> {
        timers::cancel(timer).map_err(refused)
    }

    fn open_surface(&mut self, kind: &str, title: &str) -> Result<String, Refused> {
        let kind = match kind {
            "document" => ui::SurfaceKind::Document,
            "overlay" => ui::SurfaceKind::Overlay,
            "node-3d" => ui::SurfaceKind::Node3d,
            _ => ui::SurfaceKind::Panel,
        };
        ui::open_surface(kind, title).map_err(refused)
    }

    fn close_surface(&mut self, surface: &str) -> Result<(), Refused> {
        ui::close_surface(surface).map_err(refused)
    }

    fn render(&mut self, surface: &str, tree: &str) -> Result<(), Refused> {
        ui::render(surface, tree).map_err(refused)
    }

    fn context(&mut self) -> Result<behaviour::Context, Refused> {
        Ok(behaviour::Context {
            principal: context::principal().map_err(refused)?,
            actor_chain: context::actor_chain().map_err(refused)?,
            confirmed: context::confirmed_by_human().map_err(refused)?,
        })
    }

    fn fetch(&mut self, request: &behaviour::Fetch) -> Result<behaviour::Fetched, Refused> {
        net::fetch(&net::Request {
            method: request.method.clone(),
            url: request.url.clone(),
            headers: request.headers.clone(),
            body: request.body.clone(),
        })
        .map(|response| behaviour::Fetched {
            status: response.status,
            headers: response.headers,
            body: response.body,
        })
        .map_err(refused)
    }
}

impl World {
    /// The functions of `blobs` and `commands`, which hand out resources.
    fn call_further(function: &str) -> Outcome {
        match function {
            "blobs#locate" => outcome(blobs::locate(A_HASH)),
            "blobs#open" => outcome(blobs::open(A_HASH)),
            "blobs#[method]reader.size" => on(blobs::open(A_HASH), |reader| {
                reader.size();
                Ok(())
            }),
            "blobs#[method]reader.read" => {
                on(blobs::open(A_HASH), |reader| reader.read(1).map(drop))
            }
            "blobs#[method]reader.seek" => on(blobs::open(A_HASH), |reader| reader.seek(0)),
            "blobs#create" => outcome(blobs::create()),
            "blobs#[method]writer.write" => on(blobs::create(), |writer| writer.write(b"probe")),
            "blobs#[method]writer.finish" => {
                on(blobs::create(), |writer| writer.finish().map(drop))
            }
            "blobs#announce" => outcome(blobs::announce(A_HASH)),
            "commands#dispatch" => outcome(commands::dispatch(DISPATCHED, "{}")),
            // **Each unit the probe begins it commits**, so that reaching a
            // function leaves the probe's own unit to be kept: a unit let go
            // uncommitted dooms the call tree's, and the probe would answer
            // nothing. The abort is reached on a unit committed already,
            // which it leaves as it was.
            "commands#begin" => match commands::begin("probe") {
                Ok(unit) => outcome(unit.commit()),
                Err(error) => Outcome::Refused(refused(error)),
            },
            "commands#[method]unit.compensate-with" => on(commands::begin("probe"), |unit| {
                let registered = unit.compensate_with("reference.note.get", A_NOTE);
                unit.commit().and(registered)
            }),
            "commands#[method]unit.commit" => on(commands::begin("probe"), commands::Unit::commit),
            "commands#[method]unit.abort" => on(commands::begin("probe"), |unit| {
                unit.commit()?;
                unit.abort();
                Ok(())
            }),
            other => Self::call_the_rest(other),
        }
    }

    /// The functions of `events`, `ui`, `net` and `timers`.
    fn call_the_rest(function: &str) -> Outcome {
        match function {
            "ui#open-surface" => outcome(ui::open_surface(ui::SurfaceKind::Panel, "Probe")),
            "ui#close-surface" => outcome(ui::close_surface("probe")),
            "ui#render" => outcome(ui::render("probe", r#"{"node":"divider"}"#)),
            "net#fetch" => outcome(net::fetch(&net::Request {
                method: "GET".to_owned(),
                url: "https://example.com/".to_owned(),
                headers: Vec::new(),
                body: None,
            })),
            "timers#after" => outcome(timers::after(1_000, "probe")),
            "timers#every" => outcome(timers::every(1_000, "probe")),
            "timers#cancel" => outcome(timers::cancel(0)),
            other => Outcome::Refused(Refused::NotFound(format!(
                "`{other}` is not a function the probe knows how to call"
            ))),
        }
    }
}

/// What a function that answers `result<_, error>` answered.
fn outcome<T>(answered: Result<T, Error>) -> Outcome {
    match answered {
        Ok(_) => Outcome::Answered,
        Err(error) => Outcome::Refused(refused(error)),
    }
}

/// A method called on the resource its constructor handed out, or not called
/// at all when the constructor refused.
fn on<R>(constructed: Result<R, Error>, method: impl FnOnce(&R) -> Result<(), Error>) -> Outcome {
    match constructed {
        Ok(resource) => outcome(method(&resource)),
        Err(_) => Outcome::Unreachable,
    }
}

/// One command, in the world's record.
fn descriptor(command: behaviour::Command) -> Descriptor {
    Descriptor {
        name: command.name,
        title: command.title.to_owned(),
        args_schema: command.args_schema.to_owned(),
        mutating: command.mutating,
        action: command.action.to_owned(),
        resource_selector: command.resource_selector.to_owned(),
        reversibility: match command.reversibility {
            behaviour::Reversibility::Read => Reversibility::Read,
            behaviour::Reversibility::Write => Reversibility::Write,
            behaviour::Reversibility::Irreversible => Reversibility::Irreversible,
        },
    }
}

/// The world's `error`, as the plugin's own refusal.
fn refused(error: Error) -> Refused {
    match error {
        Error::Denied(why) => Refused::Denied(why),
        Error::NotFound(why) => Refused::NotFound(why),
        Error::Invalid(why) => Refused::Invalid(why),
        Error::Conflict(why) => Refused::Conflict(why),
        Error::Unavailable(why) => Refused::Unavailable(why),
        Error::Internal(why) => Refused::Internal(why),
    }
}

/// The plugin's own refusal, in the world's `error`.
fn error(refused: Refused) -> Error {
    match refused {
        Refused::Denied(why) => Error::Denied(why),
        Refused::NotFound(why) => Error::NotFound(why),
        Refused::Invalid(why) => Error::Invalid(why),
        Refused::Conflict(why) => Error::Conflict(why),
        Refused::Unavailable(why) => Error::Unavailable(why),
        Refused::Internal(why) => Error::Internal(why),
    }
}

nc_guest::generated::export!(Plugin with_types_in nc_guest::generated);

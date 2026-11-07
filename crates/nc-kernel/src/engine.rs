//! The runtime every plugin in one kernel is compiled for and runs on, and the
//! thread its epoch is advanced by.
//!
//! **Three limits, and the engine is where two are switched on** (`NOCTUA.md
//! §11.3`). Fuel bounds how much a plugin computes and epoch interruption bounds
//! how long it runs, and both are properties of the engine: a store of an engine
//! built without them has no way to stop a plugin that spins. The third limit,
//! memory, is a limiter each plugin's store carries (`crate::limits`). The
//! engine also holds every plugin to the catalog's `wasm_stack_bytes` of native
//! stack, so one that recurses without bound traps rather than reaching the
//! host's; and no plugin has a thread of its own, because wasmtime is built
//! without its `threads` feature, so a module that shares its memory is refused
//! when it is compiled.
//!
//! **The epoch advances on a thread of its own.** A [`Ticker`] calls
//! `increment_epoch` every `epoch_tick_ms`, and a kernel holds one for as long
//! as it is open: without it the deadline every call is given is never
//! reached, which is the plugin that spins. Dropping the ticker stops its
//! thread and waits for it, so no kernel leaves one running.
//!
//! **Compiled on every load, never read back.** wasmtime can serialise a
//! compiled component and load it again, and the loading half —
//! `Component::deserialize` — is `unsafe`: it maps native code from the bytes
//! it is given, and trusts them. So [`Engine::compile`] is `Component::new`
//! over the component's own bytes, every time, and the engine is built without
//! wasmtime's on-disk cache, whose reading half is the same door.
//!
//! What this answers (`NOCTUA.md §2.4`): a file on disk cannot hand the kernel
//! machine code, a plugin compiled here cannot run without the two counters
//! that stop it or past its stack, and the epoch one of them counts advances
//! while the kernel is open. What it does not answer: how much fuel and how
//! many ticks one call is given, which each plugin's store is given per call
//! (`crate::supervisor`), and whether a tick arrives on time on a machine too
//! loaded to run the thread, which only makes a deadline later.

use core::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use nc_catalog::KERNEL;

/// The wasmtime engine, configured the one way the kernel runs plugins.
#[derive(Debug, Clone)]
pub struct Engine {
    engine: wasmtime::Engine,
}

/// A component compiled for an [`Engine`], from bytes, on this load.
#[derive(Debug, Clone)]
pub struct Compiled {
    component: wasmtime::component::Component,
}

/// Why wasmtime would not build the engine, or would not compile a component.
#[derive(Debug)]
pub struct EngineError {
    doing: &'static str,
    cause: wasmtime::Error,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wasmtime refused {}: {:#}", self.doing, self.cause)
    }
}

impl std::error::Error for EngineError {}

/// The thread that advances an engine's epoch, stopped and waited for when it
/// is dropped.
///
/// **A channel is its stop flag.** The thread waits for a message one tick at
/// a time and advances the epoch each time none came; dropping the sender
/// wakes it at once, so stopping it costs no tick, and the join that follows
/// is what makes the drop the end of it rather than a request.
#[derive(Debug)]
pub struct Ticker {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    ticks: Arc<AtomicU64>,
}

impl Ticker {
    /// How many times the thread has advanced the epoch since it started.
    #[must_use]
    pub fn advanced(&self) -> u64 {
        self.ticks.load(Ordering::Relaxed)
    }

    /// The count the thread advances, which the thread holds for as long as
    /// it runs.
    #[cfg(test)]
    fn held_by_the_thread(&self) -> std::sync::Weak<AtomicU64> {
        Arc::downgrade(&self.ticks)
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            // A thread that panicked has stopped already, which is all a drop
            // asks of it.
            let _stopped = thread.join();
        }
    }
}

impl Engine {
    /// The engine: the Component Model, fuel, epoch interruption and the
    /// catalog's stack bound.
    ///
    /// # Errors
    ///
    /// Returns an [`EngineError`] when wasmtime refuses the
    /// configuration.
    pub fn new() -> Result<Self, EngineError> {
        wasmtime::Engine::new(&configuration())
            .map(|engine| Self { engine })
            .map_err(|cause| EngineError {
                doing: "the engine's configuration",
                cause,
            })
    }

    /// Starts the thread that advances this engine's epoch.
    ///
    /// It advances it every `epoch_tick_ms`, until the [`Ticker`] is dropped.
    ///
    /// # Errors
    ///
    /// Returns an [`EngineError`] when the system
    /// refuses the thread.
    pub fn ticker(&self) -> Result<Ticker, EngineError> {
        let (stop, stopped) = mpsc::channel::<()>();
        let (engine, ticks) = (self.engine.clone(), Arc::new(AtomicU64::new(0)));
        let counted = Arc::clone(&ticks);
        let tick = Duration::from_millis(KERNEL.epoch_tick_ms);
        let thread = std::thread::Builder::new()
            .name("nc-kernel-epoch".to_owned())
            .spawn(move || {
                while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(tick) {
                    engine.increment_epoch();
                    counted.fetch_add(1, Ordering::Relaxed);
                }
            })
            .map_err(|refused| EngineError {
                doing: "the thread that advances the epoch",
                cause: wasmtime::Error::new(refused),
            })?;
        Ok(Ticker {
            stop: Some(stop),
            thread: Some(thread),
            ticks,
        })
    }

    /// Compiles a component from its bytes, which is the only way this kernel
    /// loads one.
    ///
    /// # Errors
    ///
    /// Returns an [`EngineError`] for bytes that are not a component this engine
    /// can compile.
    pub fn compile(&self, component: &[u8]) -> Result<Compiled, EngineError> {
        wasmtime::component::Component::new(&self.engine, component)
            .map(|component| Compiled { component })
            .map_err(|cause| EngineError {
                doing: "the component",
                cause,
            })
    }

    /// The wasmtime engine inside.
    pub(crate) const fn wasmtime(&self) -> &wasmtime::Engine {
        &self.engine
    }
}

impl Compiled {
    /// Every host function the component imports, as `interface#function`.
    ///
    /// **What it imports, which is less than the world offers.** A component
    /// imports only the functions its guest calls — an interface whose types it
    /// uses and whose functions it does not appears with none, and is not
    /// listed — so this is the surface a plugin can actually reach, which is
    /// the one its manifest's grants have to cover.
    ///
    /// **Every import, whatever its shape, and not the instances alone.** The
    /// kernel compiles whatever bytes it is handed, and a component need not
    /// have been built from this world: one that imports a function by itself,
    /// with no interface around it, is named here by that name alone, and a
    /// function inside an instance inside an instance by the whole path, joined
    /// with `#`. A walk of the top-level instances only would report an empty
    /// surface for a crafted component importing `foo` bare — measured with a
    /// probe encoded the way the plugin build encodes a plugin — and a link
    /// that stubbed `foo` like any other import would let it reach a function
    /// that surface does not show.
    #[must_use]
    pub fn imports(&self, engine: &Engine) -> Vec<String> {
        let mut functions = Vec::new();
        for (name, import) in self.component.component_type().imports(engine.wasmtime()) {
            functions_in(name, &import.ty, engine.wasmtime(), &mut functions);
        }
        functions
    }

    /// The wasmtime component inside.
    pub(crate) const fn wasmtime(&self) -> &wasmtime::component::Component {
        &self.component
    }
}

/// The configuration every engine of the kernel is built with.
///
/// The Component Model, fuel, epoch interruption and the catalog's
/// `wasm_stack_bytes`; the stack an asynchronous call runs on is wasmtime's
/// default, which keeps room beyond the plugin's for the host functions it
/// calls.
fn configuration() -> wasmtime::Config {
    let mut config = wasmtime::Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.max_wasm_stack(usize::try_from(KERNEL.wasm_stack_bytes).unwrap_or(usize::MAX));
    config
}

/// Every function `item` is or holds, pushed onto `into` under `path`.
///
/// A function is listed under its path; an instance is walked, and an instance
/// inside it the same way, each name joined to the path with `#`. What is
/// neither is no function a plugin can call: a type or a resource declares
/// nothing callable, and anything that is not one of the world's functions,
/// a module or a component import included, has no definition in the host
/// and is refused at the link.
fn functions_in(
    path: &str,
    item: &wasmtime::component::types::ComponentItem,
    engine: &wasmtime::Engine,
    into: &mut Vec<String>,
) {
    use wasmtime::component::types::ComponentItem;
    match item {
        ComponentItem::ComponentFunc(_) => into.push(path.to_owned()),
        ComponentItem::ComponentInstance(instance) => {
            for (name, export) in instance.exports(engine) {
                functions_in(&format!("{path}#{name}"), &export.ty, engine, into);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{Engine, configuration};
    use nc_catalog::KERNEL;

    /// A core module whose one export, `spin`, is `loop br 0 end`: a loop that
    /// never ends and calls nothing.
    const SPIN: [u8; 39] = [
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // the header
        0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // one type: [] -> []
        0x03, 0x02, 0x01, 0x00, // one function of it
        0x07, 0x08, 0x01, 0x04, b's', b'p', b'i', b'n', 0x00, 0x00, // exported as `spin`
        0x0a, 0x09, 0x01, 0x07, 0x00, 0x03, 0x40, 0x0c, 0x00, 0x0b, 0x0b, // loop br 0 end
    ];

    /// A core module whose one export, `down`, calls itself as many times as
    /// its argument says: `if n == 0 { 0 } else { down(n - 1) + 1 }`.
    const DOWN: [u8; 55] = [
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // the header
        0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f, // one type: [i32] -> [i32]
        0x03, 0x02, 0x01, 0x00, // one function of it
        0x07, 0x08, 0x01, 0x04, b'd', b'o', b'w', b'n', 0x00, 0x00, // exported as `down`
        0x0a, 0x17, 0x01, 0x15, 0x00, // its body, with no locals:
        0x20, 0x00, 0x45, 0x04, 0x7f, 0x41, 0x00, // if local 0 == 0 { 0 }
        0x05, 0x20, 0x00, 0x41, 0x01, 0x6b, 0x10, 0x00, // else { down(local 0 - 1)
        0x41, 0x01, 0x6a, 0x0b, 0x0b, // + 1 }
    ];

    /// The deepest `down` an engine configured as `config` runs without
    /// overflowing its stack, found by halving.
    fn deepest(config: &wasmtime::Config) -> u32 {
        let engine = wasmtime::Engine::new(config).expect("the configuration is valid");
        let module = wasmtime::Module::new(&engine, DOWN).expect("the module is well formed");
        let (mut runs, mut overflows) = (0_u32, 1_u32 << 22);
        while overflows - runs > 1 {
            let depth = runs + (overflows - runs) / 2;
            let mut store = wasmtime::Store::new(&engine, ());
            store.set_fuel(u64::MAX).expect("fuel is metered");
            store.set_epoch_deadline(1 << 40);
            let down = wasmtime::Instance::new(&mut store, &module, &[])
                .and_then(|instance| instance.get_typed_func::<u32, u32>(&mut store, "down"))
                .expect("`down` is exported");
            match down.call(&mut store, depth) {
                Ok(_) => runs = depth,
                Err(trap) => {
                    assert_eq!(
                        trap.downcast_ref::<wasmtime::Trap>(),
                        Some(&wasmtime::Trap::StackOverflow),
                        "{trap:?}"
                    );
                    overflows = depth;
                }
            }
        }
        runs
    }

    /// Fuel is metered: a store of this engine starts with none, and says so.
    #[test]
    fn a_store_of_this_engine_meters_fuel() {
        let engine = Engine::new().expect("the configuration is one wasmtime accepts");
        let store = wasmtime::Store::new(engine.wasmtime(), ());
        assert_eq!(
            store.get_fuel().ok(),
            Some(0),
            "an engine without fuel cannot stop a plugin that computes without bound"
        );
    }

    /// A loop that never ends is stopped by the epoch, not by the fuel.
    ///
    /// A core module written out by hand — one exported function whose body is
    /// `loop br 0 end` — run with fuel for several seconds of looping and an
    /// epoch deadline one tick away, and the engine's epoch advanced from
    /// another thread. An engine without epoch interruption spins until the
    /// fuel runs out, which is the other trap and fails the assertion.
    #[test]
    fn a_plugin_that_spins_is_interrupted_by_the_epoch() {
        let engine = Engine::new().expect("the configuration is one wasmtime accepts");
        let module =
            wasmtime::Module::new(engine.wasmtime(), SPIN).expect("the module is well formed");
        let mut store = wasmtime::Store::new(engine.wasmtime(), ());
        store.set_fuel(10_000_000_000).expect("fuel is metered");
        store.set_epoch_deadline(1);
        let instance =
            wasmtime::Instance::new(&mut store, &module, &[]).expect("the module instantiates");
        let spin = instance
            .get_typed_func::<(), ()>(&mut store, "spin")
            .expect("`spin` is exported");
        let ticker = engine.wasmtime().clone();
        let tick = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            ticker.increment_epoch();
        });
        let stopped = spin.call(&mut store, ()).expect_err("the loop never ends");
        tick.join().expect("the ticker finishes");
        assert_eq!(
            stopped.downcast_ref::<wasmtime::Trap>(),
            Some(&wasmtime::Trap::Interrupt),
            "{stopped:?}"
        );
    }

    /// The ticker advances the epoch until it is dropped, and its thread is
    /// gone once the drop returns.
    ///
    /// The loop is given fuel for seconds of looping and a deadline one tick
    /// away, so an epoch nothing advances ends in the other trap rather than
    /// in a test that never returns. What the thread holds is let go of by the
    /// time the drop returns, which the join guarantees; a drop that only let
    /// the thread go would end it microseconds later instead, so the count it
    /// holds is read the moment the drop returns — measured, with the join
    /// taken out this fails in most runs, the thread still holding it.
    #[test]
    fn the_ticker_advances_the_epoch_until_it_is_dropped() {
        let engine = Engine::new().expect("the configuration is one wasmtime accepts");
        let ticker = engine
            .ticker()
            .expect("the system gives the ticker a thread");
        let module =
            wasmtime::Module::new(engine.wasmtime(), SPIN).expect("the module is well formed");
        let mut store = wasmtime::Store::new(engine.wasmtime(), ());
        store.set_fuel(2_000_000_000).expect("fuel is metered");
        store.set_epoch_deadline(1);
        let spin = wasmtime::Instance::new(&mut store, &module, &[])
            .and_then(|instance| instance.get_typed_func::<(), ()>(&mut store, "spin"))
            .expect("`spin` is exported");
        let stopped = spin.call(&mut store, ()).expect_err("the loop never ends");
        assert_eq!(
            stopped.downcast_ref::<wasmtime::Trap>(),
            Some(&wasmtime::Trap::Interrupt),
            "the epoch did not advance: {stopped:?}"
        );
        assert!(ticker.advanced() >= 1, "{} tick(s)", ticker.advanced());
        let held = ticker.held_by_the_thread();
        drop(ticker);
        assert!(
            held.upgrade().is_none(),
            "the ticker's thread was still running once the ticker was dropped"
        );
    }

    /// A plugin is held to the catalog's stack, not to wasmtime's default.
    ///
    /// **Measured as a ratio, because a frame's size is the compiler's.** The
    /// deepest recursion the kernel's engine runs is compared with the deepest
    /// one an engine of the same configuration runs on half the stack: twice
    /// as deep is the catalog's bound applied, and the same depth is a bound
    /// nobody set.
    #[test]
    fn a_plugin_is_held_to_the_catalogs_stack() {
        let mut half = configuration();
        half.max_wasm_stack(usize::try_from(KERNEL.wasm_stack_bytes / 2).expect("fits"));
        let (catalogs, halved) = (deepest(&configuration()), deepest(&half));
        let ratio = f64::from(catalogs) / f64::from(halved);
        assert!(
            (1.8..2.2).contains(&ratio),
            "the catalog's stack reached {catalogs} calls deep and half of it {halved}"
        );
    }

    /// A module that shares its memory is refused: no plugin has a thread of
    /// its own.
    ///
    /// The control is the same memory unshared, which compiles, so what is
    /// refused is the sharing and nothing else about the module.
    #[test]
    fn a_module_that_shares_its_memory_is_refused() {
        let engine = Engine::new().expect("the configuration is one wasmtime accepts");
        let memory = |flags: u8| -> Vec<u8> {
            let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
            module.extend([0x05, 0x04, 0x01, flags, 0x01, 0x01]);
            module
        };
        assert!(
            wasmtime::Module::new(engine.wasmtime(), memory(0x01)).is_ok(),
            "a memory of one page, unshared, is a module wasmtime compiles"
        );
        let refused = wasmtime::Module::new(engine.wasmtime(), memory(0x03))
            .expect_err("a shared memory is a module with threads");
        assert!(
            format!("{refused:#}").contains("threads"),
            "refused for something other than sharing: {refused:#}"
        );
    }

    /// Bytes that are not a component are refused, and the refusal names what.
    #[test]
    fn bytes_that_are_not_a_component_are_refused() {
        let engine = Engine::new().expect("the configuration is one wasmtime accepts");
        let refused = engine
            .compile(b"not a component")
            .expect_err("text is not a component");
        assert!(
            refused
                .to_string()
                .starts_with("wasmtime refused the component: "),
            "{refused}"
        );
    }
}

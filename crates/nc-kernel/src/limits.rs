//! The bounds one plugin is held to: the catalog's, and what its manifest
//! lowers them to.
//!
//! **One source, and it can only be lowered.** Every bound the kernel holds a
//! plugin to is `nc_catalog::KERNEL`, generated from `[kernel]` in
//! `catalog/limits.toml`, and [`Limits::catalog`] reads it directly. A copy
//! of it can be edited outside `nc-catalog` — `#[non_exhaustive]` refuses a
//! struct literal and nothing else, and the fields are `pub` on a `Copy`
//! type — so what keeps the kernel from being handed a second set of numbers
//! is that it has nowhere to take one: [`Limits`] keeps its fields private,
//! and no function of this crate takes a `KernelLimits` or a
//! `CapabilitySpec`, which a test below reads every source for. Four of
//! them a plugin's manifest may lower for itself, and [`Limits::lowered`]
//! refuses any value above the catalog's: memory, fuel per dispatch, the
//! epoch deadline, and how many keys its store may keep, which is
//! `store_entries`' row in `[tables.*]` because the local database refuses
//! at it. Every other bound is the catalog's for every plugin, and the kernel
//! reads it from `KERNEL` where it applies it.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin cannot raise a limit by
//! writing a larger number in its manifest, and a limit of zero — a plugin that
//! could do nothing of what the limit counts — is refused as the mistake it
//! is. What it does not answer: whether a lowered limit is one the plugin can
//! live within, which its author finds out by being stopped.

use core::fmt;

use nc_catalog::KERNEL;
use serde::Deserialize;

/// The bounds one plugin is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    memory_bytes: u64,
    fuel_per_dispatch: u64,
    epoch_deadline_ticks: u64,
    store_entries: u64,
}

/// What a manifest's `[limits]` asks for: each bound it lowers, or nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lowering {
    /// How large the plugin's linear memory may grow, in bytes.
    pub memory_bytes: Option<u64>,
    /// How much one dispatch to it may compute, in units of fuel.
    pub fuel_per_dispatch: Option<u64>,
    /// How many ticks of the epoch one call into it may run for.
    pub epoch_deadline_ticks: Option<u64>,
    /// How many keys its store may keep.
    pub store_entries: Option<u64>,
}

/// A lowering the kernel refuses: a bound raised, or lowered to nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitRefused {
    limit: &'static str,
    asked: u64,
    ceiling: u64,
}

impl LimitRefused {
    /// The bound that was asked for, as `[limits]` writes it.
    #[must_use]
    pub const fn limit(&self) -> &'static str {
        self.limit
    }
}

impl fmt::Display for LimitRefused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.asked == 0 {
            write!(
                formatter,
                "`{}` is lowered to zero, which leaves the plugin none of what it counts",
                self.limit
            )
        } else {
            write!(
                formatter,
                "`{}` asks for {}, above the catalog's {}, and a manifest may only lower a \
                 limit",
                self.limit, self.asked, self.ceiling
            )
        }
    }
}

impl std::error::Error for LimitRefused {}

impl Limits {
    /// The catalog's bounds, which a plugin whose manifest lowers nothing has.
    ///
    /// # Panics
    ///
    /// When `catalog/limits.toml` declares no `store_entries` table with a
    /// `max_rows`, which the compiler refuses for a table that refuses at its
    /// limit — so a build of this crate holds one.
    #[must_use]
    pub fn catalog() -> Self {
        let store_entries = nc_catalog::TABLES
            .iter()
            .find(|table| table.name == "store_entries")
            .and_then(|table| table.max_rows)
            .expect("`[tables.store_entries]` declares the quota its writer refuses at");
        Self {
            memory_bytes: KERNEL.memory_bytes,
            fuel_per_dispatch: KERNEL.fuel_per_dispatch,
            epoch_deadline_ticks: KERNEL.epoch_deadline_ticks,
            store_entries,
        }
    }

    /// These bounds with every one `lowering` names lowered to it.
    ///
    /// # Errors
    ///
    /// Returns every bound `lowering` raises above this one or lowers to zero,
    /// each as a [`LimitRefused`] naming it, in the order [`Lowering`] declares
    /// them: all four are judged before any is refused, so a manifest is told
    /// everything wrong with it at once.
    pub fn lowered(self, lowering: &Lowering) -> Result<Self, Vec<LimitRefused>> {
        let mut refused = Vec::new();
        let mut one = |limit: &'static str, ceiling: u64, asked: Option<u64>| match asked {
            None => ceiling,
            Some(asked) if asked == 0 || asked > ceiling => {
                refused.push(LimitRefused {
                    limit,
                    asked,
                    ceiling,
                });
                ceiling
            }
            Some(asked) => asked,
        };
        let lowered = Self {
            memory_bytes: one("memory_bytes", self.memory_bytes, lowering.memory_bytes),
            fuel_per_dispatch: one(
                "fuel_per_dispatch",
                self.fuel_per_dispatch,
                lowering.fuel_per_dispatch,
            ),
            epoch_deadline_ticks: one(
                "epoch_deadline_ticks",
                self.epoch_deadline_ticks,
                lowering.epoch_deadline_ticks,
            ),
            store_entries: one("store_entries", self.store_entries, lowering.store_entries),
        };
        if refused.is_empty() {
            Ok(lowered)
        } else {
            Err(refused)
        }
    }

    /// How large the plugin's linear memory may grow, in bytes.
    #[must_use]
    pub const fn memory_bytes(&self) -> u64 {
        self.memory_bytes
    }

    /// How much one dispatch to the plugin may compute, in units of fuel.
    #[must_use]
    pub const fn fuel_per_dispatch(&self) -> u64 {
        self.fuel_per_dispatch
    }

    /// How many ticks of the epoch one call into the plugin may run for.
    #[must_use]
    pub const fn epoch_deadline_ticks(&self) -> u64 {
        self.epoch_deadline_ticks
    }

    /// How many keys the plugin's store may keep.
    #[must_use]
    pub const fn store_entries(&self) -> u64 {
        self.store_entries
    }
}

/// What wasmtime asks before a plugin's memory or a table of it grows, or
/// another instance, table or memory joins its store.
///
/// **Its own error, not a trap and not `Ok(false)`.** A refusal answered
/// `Ok(false)` makes the guest's `memory.grow` fail, and Rust's allocator then
/// aborts, which arrives as the same trap a plugin's own `unreachable` does:
/// nothing would tell the two apart. Answered with a [`MemoryRefused`], the
/// growth traps at once carrying that value, which is how a failure is told to
/// be the memory bound's rather than the plugin's.
///
/// **One memory per store**, so one plugin per store: a second instance with
/// a memory of its own is refused, which is what keeps one plugin's growth
/// from being counted against another's. Instances and tables are bounded by
/// the catalog, and a table's elements too, because each costs the host a
/// pointer outside the memory bound.
#[derive(Debug)]
pub(crate) struct MemoryGuard {
    memory_bytes: u64,
}

/// A growth the memory guard refused: how large the memory asked to be, and
/// the bound it is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryRefused {
    desired: u64,
    bound: u64,
}

impl fmt::Display for MemoryRefused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "its memory asked to grow to {} bytes, past the {} its `memory_bytes` allows",
            self.desired, self.bound
        )
    }
}

impl std::error::Error for MemoryRefused {}

impl MemoryGuard {
    /// A guard holding a plugin's memory to `limits`.
    pub(crate) const fn new(limits: &Limits) -> Self {
        Self {
            memory_bytes: limits.memory_bytes,
        }
    }
}

impl wasmtime::ResourceLimiter for MemoryGuard {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let desired = u64::try_from(desired).unwrap_or(u64::MAX);
        if desired > self.memory_bytes {
            return Err(MemoryRefused {
                desired,
                bound: self.memory_bytes,
            }
            .into());
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(u64::try_from(desired).unwrap_or(u64::MAX) <= u64::from(KERNEL.table_elements))
    }

    fn instances(&self) -> usize {
        usize::try_from(KERNEL.instances).unwrap_or(usize::MAX)
    }

    fn tables(&self) -> usize {
        usize::try_from(KERNEL.tables).unwrap_or(usize::MAX)
    }

    fn memories(&self) -> usize {
        1
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use super::{LimitRefused, Limits, Lowering, MemoryGuard, MemoryRefused};
    use nc_catalog::KERNEL;
    use wasmtime::ResourceLimiter as _;

    /// Growth up to the bound is allowed, and one byte past it is refused with
    /// the guard's own error.
    #[test]
    fn memory_grows_to_its_bound_and_not_one_byte_past_it() {
        let limits = Limits::catalog()
            .lowered(&Lowering {
                memory_bytes: Some(65_536 * 4),
                ..Lowering::default()
            })
            .expect("a lowering within the catalog");
        let mut guard = MemoryGuard::new(&limits);
        assert!(
            guard
                .memory_growing(0, 65_536 * 4, None)
                .expect("the bound itself is allowed")
        );
        let refused = guard
            .memory_growing(65_536 * 4, 65_536 * 4 + 1, None)
            .expect_err("one byte past the bound");
        let kept = MemoryRefused {
            desired: 65_536 * 4 + 1,
            bound: 65_536 * 4,
        };
        assert_eq!(refused.downcast_ref::<MemoryRefused>(), Some(&kept));
        assert!(
            refused.to_string().contains("`memory_bytes`"),
            "the refusal names the bound it met: {refused}"
        );
    }

    /// A table grows to the catalog's bound and no further, and a store holds
    /// the catalog's instances and tables and one memory.
    #[test]
    fn a_table_and_a_store_are_held_to_the_catalog() {
        let elements = usize::try_from(KERNEL.table_elements).expect("fits");
        let mut guard = MemoryGuard::new(&Limits::catalog());
        assert!(
            guard
                .table_growing(0, elements, None)
                .expect("an answer, never a trap")
        );
        assert!(
            !guard
                .table_growing(0, elements + 1, None)
                .expect("an answer, never a trap")
        );
        assert_eq!(
            guard.instances(),
            usize::try_from(KERNEL.instances).expect("fits")
        );
        assert_eq!(
            guard.tables(),
            usize::try_from(KERNEL.tables).expect("fits")
        );
        assert_eq!(guard.memories(), 1);
    }

    /// No source of this crate names the catalog's limits type or its
    /// capability spec, so no function of it can take either from a caller.
    ///
    /// **The guarantee the seal was taken for.** `#[non_exhaustive]` stops a
    /// struct literal and nothing else: a review compiled
    /// `let mut second = nc_catalog::KERNEL; second.memory_bytes = u64::MAX;`
    /// in a suite of this crate, and the same edit of a copied
    /// `CapabilitySpec`, so a second value of either can be made anywhere.
    /// What stops it reaching a decision is that the kernel reads `KERNEL` and
    /// `Capability::spec` itself and accepts neither type from outside: a
    /// signature has to spell the type it takes, and a generic over them has no
    /// trait to read a field through. Comments are left out of the reading, so
    /// the sentences that explain this are not what it refuses; a macro that
    /// assembles the name is what it does not see.
    ///
    /// **The walk is held to the crate's own module tree, both ways.** It was
    /// floored at `read > 1`, and a review planted a type in
    /// `crates/nc-kernel/src/host/store.rs` and a walk that stopped at the top
    /// directory: half the sources were read, the floor held, and the test
    /// passed. Every `mod name;` of a file read is a file it must read, which
    /// the compiler keeps true, so a walk that skips a directory, or reads a
    /// file the crate does not build, fails.
    #[test]
    fn no_function_of_the_kernel_takes_the_catalogs_limits_or_a_spec_from_its_caller() {
        let names = [concat!("Kernel", "Limits"), concat!("Capability", "Spec")];
        let src = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut sources = vec![src.clone()];
        let mut read: BTreeSet<PathBuf> = BTreeSet::new();
        let mut declared: BTreeSet<PathBuf> = BTreeSet::from([src.join("lib.rs")]);
        let mut naming: Vec<String> = Vec::new();
        while let Some(path) = sources.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).expect("the crate's sources are readable") {
                    sources.push(entry.expect("a directory entry").path());
                }
                continue;
            }
            if path.extension().is_none_or(|kind| kind != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source is UTF-8");
            declared.extend(modules_declared_by(&src, &path, &text));
            for (at, line) in text.lines().enumerate() {
                let code = line.split_once("//").map_or(line, |(code, _)| code);
                for name in names {
                    if code.contains(name) {
                        naming.push(format!("{}:{}: {name}", path.display(), at + 1));
                    }
                }
            }
            read.insert(path);
        }
        assert_eq!(
            read,
            declared,
            "the walk read {} source(s) and the crate's modules are {}: read and not a module: \
             {:?}; a module and not read: {:?}",
            read.len(),
            declared.len(),
            read.difference(&declared).collect::<Vec<_>>(),
            declared.difference(&read).collect::<Vec<_>>()
        );
        assert!(
            naming.is_empty(),
            "the kernel names a catalog type a caller could hand it an edited copy of: \
             {naming:?}. Read the value from `nc_catalog` where it is used rather than \
             taking one"
        );
    }

    /// The files the modules `text` declares stand in, for the source at `path`
    /// under `src`.
    ///
    /// `mod host;` in the crate's root, `crates/nc-kernel/src/lib.rs`, is
    /// `crates/nc-kernel/src/host.rs`, and `mod store;` in that is
    /// `crates/nc-kernel/src/host/store.rs`. A module written inline, with a
    /// body, is no file and is left out.
    fn modules_declared_by(src: &Path, path: &Path, text: &str) -> Vec<PathBuf> {
        let directory = if path == src.join("lib.rs") {
            src.to_path_buf()
        } else {
            path.with_extension("")
        };
        text.lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let line = line
                    .strip_prefix("pub(crate) ")
                    .or_else(|| line.strip_prefix("pub "))
                    .unwrap_or(line);
                line.strip_prefix("mod ")?.strip_suffix(';')
            })
            .map(|name| directory.join(format!("{name}.rs")))
            .collect()
    }

    #[test]
    fn the_catalogs_limits_are_the_ones_a_plugin_lowering_nothing_has() {
        let limits = Limits::catalog()
            .lowered(&Lowering::default())
            .expect("lowering nothing is not refused");
        assert_eq!(limits.memory_bytes(), KERNEL.memory_bytes);
        assert_eq!(limits.fuel_per_dispatch(), KERNEL.fuel_per_dispatch);
        assert_eq!(limits.epoch_deadline_ticks(), KERNEL.epoch_deadline_ticks);
        assert_eq!(
            Some(limits.store_entries()),
            nc_catalog::TABLES
                .iter()
                .find(|table| table.name == "store_entries")
                .and_then(|table| table.max_rows)
        );
    }

    #[test]
    fn a_limit_can_be_lowered_to_its_ceiling_and_below_it() {
        let ceiling = Limits::catalog();
        let lowered = ceiling
            .lowered(&Lowering {
                memory_bytes: Some(4 << 20),
                fuel_per_dispatch: Some(ceiling.fuel_per_dispatch()),
                epoch_deadline_ticks: Some(1),
                store_entries: Some(10),
            })
            .expect("each is at or below the catalog's");
        assert_eq!(
            (
                lowered.memory_bytes(),
                lowered.fuel_per_dispatch(),
                lowered.epoch_deadline_ticks(),
                lowered.store_entries()
            ),
            (4 << 20, ceiling.fuel_per_dispatch(), 1, 10)
        );
    }

    #[test]
    fn a_limit_raised_by_one_or_lowered_to_zero_is_refused_naming_it() {
        let ceiling = Limits::catalog();
        for (limit, raised) in [
            (
                "memory_bytes",
                Lowering {
                    memory_bytes: Some(ceiling.memory_bytes() + 1),
                    ..Lowering::default()
                },
            ),
            (
                "fuel_per_dispatch",
                Lowering {
                    fuel_per_dispatch: Some(ceiling.fuel_per_dispatch() + 1),
                    ..Lowering::default()
                },
            ),
            (
                "epoch_deadline_ticks",
                Lowering {
                    epoch_deadline_ticks: Some(ceiling.epoch_deadline_ticks() + 1),
                    ..Lowering::default()
                },
            ),
            (
                "store_entries",
                Lowering {
                    store_entries: Some(ceiling.store_entries() + 1),
                    ..Lowering::default()
                },
            ),
        ] {
            let refused = ceiling.lowered(&raised).expect_err("raising is refused");
            let [refused] = refused.as_slice() else {
                panic!("one bound raised, and {refused:?} refused");
            };
            assert_eq!(refused.limit(), limit);
            assert!(refused.to_string().contains("may only lower"), "{refused}");
        }
        let refused = ceiling
            .lowered(&Lowering {
                store_entries: Some(0),
                ..Lowering::default()
            })
            .expect_err("zero is refused");
        let [refused] = refused.as_slice() else {
            panic!("one bound lowered to zero, and {refused:?} refused");
        };
        assert_eq!(refused.limit(), "store_entries");
        assert!(refused.to_string().contains("to zero"), "{refused}");
    }

    /// Every bound a lowering refuses is named, not the first of them.
    ///
    /// **A review raised two limits in one manifest and was told of one:** this
    /// returned on the first refusal, and the manifest's module says every
    /// problem is reported. Here all four are wrong at once, three raised and
    /// one lowered to zero, and all four are named, in the order [`Lowering`]
    /// declares them.
    #[test]
    fn every_bound_a_lowering_refuses_is_named_not_the_first() {
        let ceiling = Limits::catalog();
        let refused = ceiling
            .lowered(&Lowering {
                memory_bytes: Some(ceiling.memory_bytes() + 1),
                fuel_per_dispatch: Some(0),
                epoch_deadline_ticks: Some(ceiling.epoch_deadline_ticks() + 1),
                store_entries: Some(ceiling.store_entries() + 1),
            })
            .expect_err("every bound is refused");
        assert_eq!(
            refused.iter().map(LimitRefused::limit).collect::<Vec<_>>(),
            [
                "memory_bytes",
                "fuel_per_dispatch",
                "epoch_deadline_ticks",
                "store_entries"
            ]
        );
    }
}

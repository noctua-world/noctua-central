//! What a plugin may call: the capabilities it holds, the refusal of the ones
//! it does not, and the record of every host function it reached.
//!
//! **Deny by default** (`NOCTUA.md §11.3`). A plugin holds the capabilities
//! its manifest asks for and nothing else — a [`GrantSet`] — and every host
//! function of the world names the one capability it answers to, as a variant
//! of `nc_catalog::Capability`, which `catalog/capabilities.toml` generates. A
//! function whose capability is not held is refused with a [`Denial`],
//! `[NC-PLUG-0001]`, which reaches the plugin as the world's `denied` value: a
//! refusal it reads and goes on from, never a trap, because a trap poisons the
//! plugin's whole store and would turn every `denied` into a kill.
//!
//! **Every function that is reached is recorded, so one that answered without
//! asking is found.** Asking is the first statement of every host function,
//! and the asking writes down which function of which interface answered to
//! which capability, as a [`Linked`]. The coverage test
//! (`crates/nc-kernel/tests/capabilities.rs`) runs the reference plugin's probe,
//! which calls every function of the world, and holds that record to the
//! catalog and to the world function for function: a host function that never
//! asked is one the record does not hold.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin reaches no capability it was
//! not granted through the world, and is told so rather than killed for it.
//! What it does not answer: what a granted function may then do, which is each
//! host function's own, and whether a manifest asked for what its plugin
//! needs, which is its author's.

use core::fmt;
use std::collections::BTreeSet;

use nc_catalog::{Capability, ErrorCode};

/// The capabilities one plugin was granted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantSet {
    granted: BTreeSet<Capability>,
}

impl GrantSet {
    /// No capability at all, which is what a manifest asking for nothing holds.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Exactly `capabilities`, each once.
    #[must_use]
    pub fn of(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            granted: capabilities.into_iter().collect(),
        }
    }

    /// Every capability the catalog declares.
    #[must_use]
    pub fn every() -> Self {
        Self::of(nc_catalog::CAPABILITIES.iter().map(|spec| spec.capability))
    }

    /// Whether `capability` was granted.
    #[must_use]
    pub fn holds(&self, capability: Capability) -> bool {
        self.granted.contains(&capability)
    }

    /// Every capability granted, in the catalog's name order.
    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        self.granted.iter().copied()
    }

    /// Whether `function`, which answers to `capability`, may run.
    ///
    /// # Errors
    ///
    /// Returns the [`Denial`] of `function` when `capability` is
    /// not granted.
    pub fn check(&self, capability: Capability, function: &'static str) -> Result<(), Denial> {
        if self.holds(capability) {
            Ok(())
        } else {
            Err(Denial {
                capability,
                function,
            })
        }
    }
}

/// One host function a plugin reached, and what it answered to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Linked {
    /// The interface of the world the function is in.
    pub interface: &'static str,
    /// The function, as the world's parser names it: `get`, or for a
    /// resource's method `[method]reader.read`.
    pub function: &'static str,
    /// The capability it answers to, or nothing for a function of an interface
    /// every plugin is given.
    pub capability: Option<Capability>,
}

/// The refusal of a host function whose capability was not granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denial {
    capability: Capability,
    function: &'static str,
}

impl Denial {
    /// The capability that was not granted.
    #[must_use]
    pub const fn capability(&self) -> Capability {
        self.capability
    }

    /// The function that was refused, as the world's parser names it.
    #[must_use]
    pub const fn function(&self) -> &'static str {
        self.function
    }

    /// The code every denial carries.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        ErrorCode::PluginCapabilityDenied
    }
}

impl fmt::Display for Denial {
    /// The sentence the plugin is answered with, opening on the code.
    ///
    /// **The code is written here, where the refusal is made**, not read back
    /// from [`Denial::code`]: this sentence is what reaches the plugin, and the
    /// code in it is the one the kernel raises.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let spec = self.capability.spec();
        write!(
            formatter,
            "[{}] `{}#{}` answers to the capability `{}`, which this plugin was not granted",
            ErrorCode::PluginCapabilityDenied,
            spec.interface,
            self.function,
            spec.name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Denial, GrantSet};
    use nc_catalog::{CAPABILITIES, Capability, ErrorCode};

    #[test]
    fn a_capability_not_granted_is_refused_and_one_granted_is_not() {
        let grants = GrantSet::of([Capability::Store]);
        assert_eq!(grants.check(Capability::Store, "get"), Ok(()));
        assert_eq!(
            grants.check(Capability::Net, "fetch"),
            Err(Denial {
                capability: Capability::Net,
                function: "fetch",
            })
        );
    }

    #[test]
    fn nothing_granted_refuses_every_capability_and_everything_granted_refuses_none() {
        for spec in CAPABILITIES {
            assert!(
                GrantSet::none()
                    .check(spec.capability, "probe")
                    .is_err_and(|denial| denial.capability() == spec.capability),
                "`{}` was not refused with nothing granted",
                spec.name
            );
            assert_eq!(
                GrantSet::every().check(spec.capability, "probe"),
                Ok(()),
                "`{}` was refused with everything granted",
                spec.name
            );
        }
        assert_eq!(
            GrantSet::every().iter().count(),
            CAPABILITIES.len(),
            "every capability the catalog declares is one a grant can hold"
        );
        assert!(
            !CAPABILITIES.is_empty(),
            "the catalog declares no capability"
        );
    }

    #[test]
    fn a_denial_names_its_code_the_function_and_the_capability() {
        let denial = GrantSet::none()
            .check(Capability::BlobsRead, "[method]reader.read")
            .expect_err("nothing is granted");
        assert_eq!(denial.code(), ErrorCode::PluginCapabilityDenied);
        assert_eq!(denial.function(), "[method]reader.read");
        assert_eq!(
            denial.to_string(),
            format!(
                "[{}] `blobs#[method]reader.read` answers to the capability `blobs.read`, which \
                 this plugin was not granted",
                ErrorCode::PluginCapabilityDenied
            )
        );
    }

    #[test]
    fn a_grant_holds_each_capability_once_in_the_catalogs_order() {
        let grants = GrantSet::of([Capability::Ui, Capability::Context, Capability::Ui]);
        assert_eq!(
            grants.iter().collect::<Vec<_>>(),
            [Capability::Context, Capability::Ui]
        );
        assert!(grants.holds(Capability::Ui) && !grants.holds(Capability::Net));
    }
}

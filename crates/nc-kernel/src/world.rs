//! The world the kernel hosts: which version, and which bytes.
//!
//! **The kernel compiles against a copy, and a copy is held twice.** The
//! normative world is `reference/noctua-kernel.wit` (`NOCTUA.md §0.4`), in the
//! workspace above this repository, which a checkout does not have — so the
//! host's bindings, and every plugin's through `crates/nc-guest`, are generated
//! from `crates/nc-kernel/wit/noctua-kernel.wit`, a byte-identical copy.
//! `the_kernel_world_is_the_reference_world` in `xtask/src/check.rs` holds the
//! two together wherever a workspace is there; [`WIT_BLAKE3`] holds the copy to
//! the bytes it had when the digest was written, so an edit to the copy alone
//! fails in a checkout too, where the other test stands aside.
//!
//! The order of an edit is the one the reference's own header gives: change
//! the workspace's file first, copy it here, then write the new digest. A copy
//! edited here, digest and all, passes this file and is refused by the
//! workspace test.

/// The version of the world, as its `package` line declares it.
pub const WORLD_VERSION: &str = "0.2.0";

/// BLAKE3 of `crates/nc-kernel/wit/noctua-kernel.wit`, in lowercase hex.
pub const WIT_BLAKE3: &str = "e58f10502efd74e3c68a9b899663bc819c1c1f4ee72781b756d93749308be604";

#[cfg(test)]
mod tests {
    /// The digest is the digest of the file the bindings are generated from.
    #[test]
    fn the_pinned_digest_is_the_worlds_on_disk() {
        let bytes = include_bytes!("../wit/noctua-kernel.wit");
        assert_eq!(
            blake3::hash(bytes).to_hex().as_str(),
            super::WIT_BLAKE3,
            "crates/nc-kernel/wit/noctua-kernel.wit is not the bytes `WIT_BLAKE3` was \
             written for: change `reference/noctua-kernel.wit` in the workspace, copy it \
             here, and write the new digest"
        );
    }

    /// The version is the one the world's `package` line declares.
    #[test]
    fn the_version_is_the_one_the_package_line_declares() {
        let mut resolve = wit_parser::Resolve::default();
        let (package, _) = resolve
            .push_path(concat!(env!("CARGO_MANIFEST_DIR"), "/wit"))
            .expect("the world parses");
        let name = &resolve.packages[package].name;
        assert_eq!(
            (name.namespace.as_str(), name.name.as_str()),
            ("noctua", "kernel"),
            "the world is not `noctua:kernel`"
        );
        assert_eq!(
            name.version.as_ref().map(ToString::to_string).as_deref(),
            Some(super::WORLD_VERSION),
            "the world's package line and `WORLD_VERSION` disagree"
        );
    }
}

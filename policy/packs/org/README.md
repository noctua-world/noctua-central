# The `org` pack — not implemented

Empty on purpose, and `NOCTUA.md §14.4` is why: the pack indirection **must
exist from the first commit, or the other two become a rewrite**. A directory
that exists is a reminder that `family` is *a* profile rather than *the*
behaviour; a directory created later is a refactor of everything that assumed
there was only one.

`reference/identity-and-authorization.md §3` describes what will go here: strict
separation of duties, JIT mandatory for production, **no age attributes**, high
quorum, long audit retention.

This directory **is** loaded: `nc_policy::policy_files` walks the whole of
`policy/`, so a rule placed here is parsed, validated against the schema and
subject to every gate — `§14.9.7`'s included, which fails the build naming a
policy with no denial test. The first rule here also brings the pack's
manifest, a `pack.toml` holding `version = <n>` and nothing else, and every
realm in `policy/world.json` whose profile is `org` pinned to `org@<n>`:
without the manifest the pack is refused for having no version, and without
the pin the realm is refused for pinning another. Until then `office`'s pin
names a pack this tree does not carry: the gate holds it to its own profile,
`org@…`, and nothing on disk holds the version. Measured
with a one-line `forbid` dropped in: the manifest is what refuses it first, then
the pin, and once both are in place the rest of the gate does, `has no denial
test` among its refusals. The verdicts are not quoted here — this repository's own
rule is that a transcript pasted into a document is a measurement of the day it
was pasted.

What `cargo xtask check` does not fail for is the *emptiness* — a pack with no policies is a pack with nothing to test, and inventing
one to satisfy a gate would put untested rules in front of a real deployment.

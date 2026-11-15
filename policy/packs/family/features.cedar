// The `family` profile's permits over the platform's features.
//
// A feature of the platform is what a command of the kernel's own selects when
// it reads none of the realm's data: `feature:<id>` in `catalog/commands.toml`,
// a `Feature` the realm's directory holds (`reference/identity-and-authorization.md
// §4`). Only the kernel's own commands may select one — the kernel refuses the
// form in a plugin's declaration at `activate` — so a permit here is a permit
// for the kernel's commands on that feature and nothing a plugin wrote, and the
// journal's entity says which feature was used.
//
// **One feature per permit, named.** Each names its feature by its id rather
// than reading an attribute such as `tier`, which is the narrowest default
// deny there is and invents no meaning for an attribute; and the deciding `@id`
// the journal records then says what was allowed.
//
// **Realm scoping is not checked here, deliberately**, for the reason
// `permissions.cedar` gives: the decision point hands Cedar the asking realm's
// entities and no other's, and denies a principal or a resource outside them
// (`NOCTUA.md §14.3`). A feature is not something a person is `in`, so a
// membership test here would have to re-derive the feature's realm — a second
// answer to a question the slice already answered. Cedar alone would allow a
// person of another realm, which `tests/features.toml` pins as it stands.

// Every person of the household names files.
//
// `naming.propose` and `naming.read` are the one file-naming scheme as commands
// (`NOCTUA.md §16.3`): they read nothing of the realm's and change nothing, so
// anybody who saves a file may ask for its name. **A person, and only a person:
// `principal is User` is this permit's own constraint.** A device and an agent
// are in the realm as much as a person is, and are refused until Stage 15 gives
// an agent a permit of its own, intersected with its operator's (§14.6). A
// frozen account is refused by `family.a_frozen_account_acts_on_nothing`, which
// beats this permit as every `forbid` does.
@id("family.a_member_names_files")
permit (
  principal is User,
  action == Action::"Read",
  resource == Feature::"naming"
);

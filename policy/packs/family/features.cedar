// The `family` profile's permits over the platform's features.
//
// A feature of the platform is what a command of the kernel's own selects when
// it reads none of the realm's data: `feature:<id>` in `catalog/commands.toml`,
// a `Feature` the realm's directory holds (`reference/identity-and-authorization.md
// §4`). Only the kernel's own commands may select one — the kernel refuses the
// `feature:` form in a plugin's declaration at `activate`, and the judge every
// declaration is held to refuses an argument naming `Feature`, so no caller
// chooses the feature — so a permit here is a permit for the kernel's commands
// on that feature and nothing a plugin wrote, and the journal's entity says
// which feature was used.
//
// **One feature per permit, named.** Each names its feature by its id rather
// than reading an attribute such as `tier`, which is the narrowest default
// deny there is and invents no meaning for an attribute; and the deciding `@id`
// the journal records then says what was allowed.
//
// **An agent's permit beside the person's.** A feature an agent may reach has
// a second permit, `principal is Agent`, which is the agent's half of what it
// is answered; the person's permit is the other half, asked of the same
// request as the agent's operator (`NOCTUA.md §14.6`). Neither permit reads the
// other: the intersection is the decision point's, which asks once for each
// and takes the narrower answer, and no policy can (`../../core/README.md`).
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
// `principal is User` is this permit's own constraint.** A device is in the
// realm as much as a person is, and is refused; an agent has a permit of its
// own, below, and is answered by both. A frozen account is refused by
// `family.a_frozen_account_acts_on_nothing`, which beats this permit as every
// `forbid` does — asked as an agent's operator too.
@id("family.a_member_names_files")
permit (
  principal is User,
  action == Action::"Read",
  resource == Feature::"naming"
);

// An agent of the house names files for the person it acts for.
//
// An agent has an identity of its own and never inherits its operator's
// privileges (`NOCTUA.md §14.6`), so the person's permit above never reaches
// it: this is the agent's own. **It is half of what an agent is answered** —
// the decision point asks the same request again as the agent's operator,
// marked as an agent's, and allows only what both allow — so an agent names a
// file only while the person it acts for may, and the agent of a frozen
// account is refused by `family.a_frozen_account_acts_on_nothing` in that
// second question. `principal is Agent` is this permit's own constraint, and a
// device is refused by it. The core refuses an agent not marked as one
// (`core.an_agent_never_impersonates`), one with nothing left to spend
// (`core.budget_is_a_permission`), one too far down a chain
// (`core.chain_depth_is_bounded`) and one its person granted nothing on this
// feature (`core.provenance`), here as for every request. No attribute of the
// agent is read: what it may do is its permits and its grants.
@id("family.an_agent_names_files")
permit (
  principal is Agent,
  action == Action::"Read",
  resource == Feature::"naming"
);

// Every person of the household finds commands.
//
// `kernel.find` is the registry filtered by the one fuzzy core
// (`NOCTUA.md §12.4`): it reads the commands' names and tags, which
// `Request::Registry` already lists to the same person, and nothing of the
// realm's, and changes nothing, so anybody who can run a command may look one
// up. **A person, and only a person: `principal is User` is this permit's own
// constraint**, for the reason the naming permit gives — a device is refused,
// and an agent has a permit of its own, below. A frozen account is refused by
// `family.a_frozen_account_acts_on_nothing`, as for every permit.
@id("family.a_member_finds_commands")
permit (
  principal is User,
  action == Action::"Read",
  resource == Feature::"registry"
);

// An agent of the house finds commands for the person it acts for.
//
// The agent's half of `kernel.find`, for the reasons the naming permit above
// gives, and asked again as the agent's operator. What a search lists is the
// kernel's to decide (`crates/nc-kernel/src/find.rs`), not this permit's: a
// policy decides one request, and a list is many.
@id("family.an_agent_finds_commands")
permit (
  principal is Agent,
  action == Action::"Read",
  resource == Feature::"registry"
);

// Every person of the household switches MCP for their own channel.
//
// `settings.mcp.read` and `settings.mcp.set` are the channel's setting of
// whether its daemon lets an MCP client in (`NOCTUA.md §12.15`, §20.3): MCP is
// off until a person turns it on. **A person, and only a person: `principal is
// User` is this permit's own constraint.** An agent that could write this
// feature could let itself or another agent in, so it is refused here, and a
// device is no person. A child of the house as much as an adult. A frozen
// account is refused by `family.a_frozen_account_acts_on_nothing`, as for every
// permit. A daemon decides only as its profile's person, so the channel
// switched is always the asker's own; and the set runs only at the top of a
// call tree, so no plugin, timer or event switches it in their name
// (`crates/nc-kernel/src/registry.rs`).
@id("family.a_member_switches_mcp")
permit (
  principal is User,
  action in [Action::"Read", Action::"Write"],
  resource == Feature::"mcp"
);

// Every person of the household keeps their own agents.
//
// `agents.create`, `agents.list`, `agents.grant` and `agents.revoke` are a
// person's delegation to an agent (`NOCTUA.md §14.6`): what they make, see,
// give and end. Delegating one's own rights grants nothing beyond them — every
// decision for the agent is asked again as its person, and the narrower answer
// stands — and only the agent's person grants it or ends a grant, which the
// directory refuses to anyone else when the change is made, so this is not
// `family.a_guardian_administers_the_realm`. **A person, and only a person:
// `principal is User` is this permit's own constraint.** An agent manages no
// agent, its own included: no permit names `Agent` on this feature, so the
// agent's half of every question about it is refused; and a device is no
// person. A child of the house as much as an adult, for agents of their own. A
// frozen account is refused by `family.a_frozen_account_acts_on_nothing`, as
// for every permit. The three commands that change run only at the top of a
// call tree, so no plugin, timer or event makes, grants or ends an agent in a
// person's name (`crates/nc-kernel/src/registry.rs`).
@id("family.a_member_keeps_their_agents")
permit (
  principal is User,
  action in [Action::"Read", Action::"Write", Action::"Grant", Action::"Revoke"],
  resource == Feature::"agents"
);

// Every person of the household drives their own interface.
//
// The commands of `shell.`, `window.`, `leader.`, `page.`, `drawer.`,
// `sidebar.`, `footer.`, `info.` and `prompt.` are the desktop interface's own
// actions (`NOCTUA.md §12.1`): which windows show, which pages are open in
// them and in the drawer, what the prompt holds and where each seat is — what
// the person's own windows show, and nothing of the realm's data. `Read`
// answers that state and `Write` changes it. So are the reads of what waits
// for the person's confirmation (`confirmations.`), of the surfaces plugins
// show and who rendered each (`surfaces.`), and of the step their next undo
// takes back (`kernel.history`); and a click on a surface, `surfaces.activate`,
// is `Write` here, the command it runs decided again as itself. So are the
// five reads of how the platform on their machine stands, which the dev pages
// show (`dev.limits.read`, `dev.plugins.read`, `dev.sessions.read`,
// `dev.agents.read`, `dev.policy.read`; `NOCTUA.md §12.16`): counts, names
// and versions, and nothing a person wrote. **A person, and only a person:
// `principal is User` is this permit's own constraint.** An agent drives no
// interface — no permit names `Agent` on this feature, so the agent's half of
// every question about it is refused, and a person's grant of it to an agent
// allows nothing — and a device is no person. A child of the house as much as
// an adult. A frozen account is refused by
// `family.a_frozen_account_acts_on_nothing`, as for every permit. A daemon
// decides only as its profile's person, so the interface driven is always the
// asker's own.
@id("family.a_member_drives_the_interface")
permit (
  principal is User,
  action in [Action::"Read", Action::"Write"],
  resource == Feature::"interface"
);

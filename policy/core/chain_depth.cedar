// `NOCTUA.md §14.6`, third rule: **`actor_chain_depth` is limited to 2.**
//
// "Agent calling agent calling agent is how privilege leaks unnoticed." The
// number was in `reference/identity-and-authorization.md §7`, in the schema at
// `policy/schema.cedarschema`, and in every case file's `[default.context]` — and
// in **no policy at all**, so nothing enforced it.
//
// Criterion 11 structurally cannot find that: it requires every core policy to
// have a denial test, and a rule with no policy has no policy to lack a test.
// `policy/core/README.md` presents itself as the complete record of what the
// core does not yet reach, and did not list it either.
//
// Two, not three: a person acts through an agent, and that agent may call one
// more. Beyond that nobody can say whose authority is being spent.
@id("core.chain_depth_is_bounded")
forbid (
  principal,
  action,
  resource
)
when { !(context has actor_chain_depth) || context.actor_chain_depth > 2 };

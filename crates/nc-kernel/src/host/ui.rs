//! `ui`: describing what a plugin shows, for the host to render.
//!
//! Gated by `ui`. Every function asks it first and answers `denied` to a
//! plugin not granted it; to one that is, it opens, closes or renders into a
//! surface the plugin holds, under the frame of the call running now —
//! refused with `[NC-PLUG-0025]` where none runs — and a tree is judged
//! against the closed set of nodes and the registry before the surface keeps
//! it (`crate::ui`). A surface the plugin does not hold is not found, and a
//! tree into one is not judged. A command that only reads may render: what a
//! surface shows is not state the undo history keeps. **What it does is held
//! by the call tree, not shown** (`crate::ui::Shown`): in the tree's unit,
//! shown when the unit is kept, and in a tree that holds none, shown once it
//! ends with every entry it gave the journal taken — so a surface never shows
//! what a command did that the journal did not take, as no event is heard
//! and no timer armed for one. Each change is judged against what is shown
//! and what the tree holds already: a surface the tree opened is its plugin's
//! and counts against its bound, and one it closed is not found.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin drawing what the host does
//! not render, or into another's surface. What it does not answer: the click,
//! which a renderer dispatches through the decision point, and which no
//! renderer in this build dispatches.

use nc_catalog::Capability;

use super::{HostState, refused};
use crate::Environment;
use crate::bindings::KernelError;
use crate::bindings::noctua::kernel::ui as world;
use crate::dispatch::lock;
use crate::ui::{Shown, SurfaceKind};

/// The world's kind of surface, as the kernel keeps it.
const fn kind(kind: world::SurfaceKind) -> SurfaceKind {
    match kind {
        world::SurfaceKind::Panel => SurfaceKind::Panel,
        world::SurfaceKind::Document => SurfaceKind::Document,
        world::SurfaceKind::Overlay => SurfaceKind::Overlay,
        world::SurfaceKind::Node3d => SurfaceKind::Node3d,
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "the world declares these async, and a surface and the tree it shows are kept in \
              the kernel's memory, judged against the registry it holds: none of them waits"
)]
impl<E: Environment> world::Host for HostState<E> {
    async fn open_surface(
        &mut self,
        surface: world::SurfaceKind,
        title: String,
    ) -> Result<String, KernelError> {
        self.require(Capability::Ui, "open-surface")?;
        let (frame, kernel) = self.acting("ui#open-surface")?;
        let mut tree = frame.tree();
        let opened = lock(&kernel.surfaces)
            .open(frame.slot(), kind(surface), &title, &tree.surfaces_held())
            .map_err(|refusal| refused(&refusal))?;
        let id = opened.id().to_owned();
        tree.later_mut().show(Shown::Opened(opened));
        Ok(id)
    }

    async fn close_surface(&mut self, id: String) -> Result<(), KernelError> {
        self.require(Capability::Ui, "close-surface")?;
        let (frame, kernel) = self.acting("ui#close-surface")?;
        let owner = frame.slot();
        let mut tree = frame.tree();
        if !lock(&kernel.surfaces).holds(owner, &id, &tree.surfaces_held()) {
            return Err(refused(&crate::ui::not_held(&id)));
        }
        tree.later_mut().show(Shown::Closed { owner, id });
        Ok(())
    }

    async fn render(&mut self, id: String, tree: String) -> Result<(), KernelError> {
        self.require(Capability::Ui, "render")?;
        let (frame, kernel) = self.acting("ui#render")?;
        let owner = frame.slot();
        // **Held before the tree is judged**, so a surface another plugin holds
        // is not found whatever was sent into it, rather than answered with
        // what is wrong with a tree it may never render.
        let holds = {
            let call_tree = frame.tree();
            lock(&kernel.surfaces).holds(owner, &id, &call_tree.surfaces_held())
        };
        if !holds {
            return Err(refused(&crate::ui::not_held(&id)));
        }
        let kept = crate::ui::validated(&tree, &|name| kernel.command(name))
            .map_err(|refusal| refused(&refusal))?;
        frame.tree().later_mut().show(Shown::Rendered {
            owner,
            id,
            tree: kept,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::kind;
    use crate::bindings::noctua::kernel::ui::{Host as _, SurfaceKind};
    use crate::capability::GrantSet;
    use crate::host::HostState;
    use crate::host::tests::answered_outside_a_tree;
    use nc_catalog::Capability;

    #[tokio::test]
    async fn every_function_is_denied_without_the_capability_and_refused_outside_a_call_tree() {
        for granted in [false, true] {
            let mut state = HostState::in_memory(if granted {
                GrantSet::of([Capability::Ui])
            } else {
                GrantSet::none()
            });
            let answers = [
                state
                    .open_surface(SurfaceKind::Panel, "Notes".to_owned())
                    .await
                    .map(drop),
                state.close_surface("s".to_owned()).await,
                state.render("s".to_owned(), "{}".to_owned()).await,
            ];
            for answer in &answers {
                assert!(answered_outside_a_tree(answer, granted), "{answer:?}");
            }
            assert_eq!(state.reached().len(), answers.len());
        }
    }

    /// A tree that renders into a surface takes the place of the one its call
    /// tree holds for that surface, and a close takes away the one held for
    /// what it closes, so what the kernel holds for one plugin's renders is a
    /// tree per surface it holds, however often it renders.
    ///
    /// **What a tree that held every render would cost**: held until the tree
    /// ends, every render of a plugin rendering a tree near `payload_bytes`
    /// into one surface in a loop — each render waiting on nothing and costing
    /// it little fuel, so only the tree's deadline ends it — would make the
    /// kernel hold gigabytes for it. The second half is the same loop through a
    /// surface opened and closed each time, which a close that left its
    /// surface's tree held would let grow as far.
    #[tokio::test]
    async fn one_plugins_renders_in_one_call_tree_hold_a_tree_per_surface_it_holds() {
        use std::sync::{Arc, Mutex};

        use crate::dispatch::{Authority, Call, CallTree, Frame, lock};
        use crate::ui::Shown;

        /// How many times the plugin renders, in each half.
        const RENDERS: usize = 6;
        /// A valid tree near the payload's bound, told from the others by
        /// `mark`.
        fn near_the_bound(mark: usize) -> String {
            let text = format!(r#"{{"node":"text","text":"{}"}}"#, "x".repeat(4_000));
            let fits = usize::try_from(nc_catalog::KERNEL.payload_bytes).expect("fits")
                / (text.len() + 1)
                - 2;
            format!(
                r#"{{"children":[{{"node":"text","text":"{mark}"}},{}],"node":"column"}}"#,
                vec![text; fits].join(",")
            )
        }
        /// The trees the call tree holds rendered, each with its surface.
        fn rendered(tree: &Mutex<CallTree>) -> Vec<(String, String)> {
            lock(tree)
                .surfaces_held()
                .into_iter()
                .filter_map(|shown| match shown {
                    Shown::Rendered { id, tree, .. } => Some((id.clone(), tree.clone())),
                    Shown::Opened(_) | Shown::Closed { .. } => None,
                })
                .collect()
        }
        let (kernel, ana) = crate::host::tests::a_kernel_for_ana().await;
        let tree = Arc::new(Mutex::new(CallTree::new(
            ana,
            nc_ids::Clock::now_ms(&nc_ids::SystemClock),
        )));
        let mut state = HostState::in_memory(GrantSet::of([Capability::Ui]));
        state.enter(Frame::new(
            Arc::downgrade(&kernel.shared),
            Arc::clone(&tree),
            crate::PluginSlot(0),
            Call::Command { mutating: false },
            Authority {
                command: "tests.door".to_owned(),
                action: "Read".to_owned(),
                resource: nc_authz::EntityRef {
                    type_name: "Content".to_owned(),
                    id: "photo".to_owned(),
                },
            },
            false,
        ));
        let id = state
            .open_surface(SurfaceKind::Panel, "Notes".to_owned())
            .await
            .expect("a surface opens");
        for mark in 0..RENDERS {
            let big = near_the_bound(mark);
            assert!(
                u64::try_from(big.len()).expect("fits") <= nc_catalog::KERNEL.payload_bytes,
                "{}",
                big.len()
            );
            state
                .render(id.clone(), big)
                .await
                .unwrap_or_else(|refused| panic!("render {mark} was refused: {refused:?}"));
        }
        let held = rendered(&tree);
        assert_eq!(
            held.iter().map(|(into, _)| into).collect::<Vec<_>>(),
            [&id],
            "{RENDERS} renders into one surface left {} tree(s) held for it",
            held.len()
        );
        assert!(
            held[0].1.contains(&format!(r#""text":"{}""#, RENDERS - 1)),
            "the tree held is not the last one rendered"
        );
        state
            .close_surface(id)
            .await
            .expect("the surface it holds closes");
        for mark in 0..RENDERS {
            let opened = state
                .open_surface(SurfaceKind::Panel, "Notes".to_owned())
                .await
                .unwrap_or_else(|refused| panic!("open {mark} was refused: {refused:?}"));
            state
                .render(opened.clone(), near_the_bound(mark))
                .await
                .unwrap_or_else(|refused| panic!("render {mark} was refused: {refused:?}"));
            state
                .close_surface(opened)
                .await
                .expect("the surface it opened closes");
        }
        let held = rendered(&tree);
        assert!(
            held.is_empty(),
            "every surface rendered into was closed, and {} tree(s) are still held",
            held.len()
        );
    }

    #[test]
    fn every_kind_of_surface_of_the_world_is_kept_as_its_own() {
        use crate::ui::SurfaceKind as Kept;
        for (world, kept, word) in [
            (SurfaceKind::Panel, Kept::Panel, "panel"),
            (SurfaceKind::Document, Kept::Document, "document"),
            (SurfaceKind::Overlay, Kept::Overlay, "overlay"),
            (SurfaceKind::Node3d, Kept::Node3d, "node-3d"),
        ] {
            assert_eq!(kind(world), kept);
            assert_eq!(kept.word(), word);
        }
    }
}

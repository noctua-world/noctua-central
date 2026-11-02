//! Events: what a plugin announces, held with the unit it was announced in,
//! queued, and delivered by the pump to every plugin that subscribed.
//!
//! **A topic is the publisher's** (`NOCTUA.md §11.3`). An event is published
//! under lower-case words joined by dots — at least two, at most
//! [`TOPIC_MAX`] characters — beginning with the namespace the plugin's
//! manifest names, so a subscriber's pattern says whose events it hears and no
//! plugin speaks in another's name; anything else is refused with
//! `[NC-PLUG-0026]`. Its data is a payload, canonical JSON within
//! `payload_bytes`. A subscriber hears a topic its activation subscribed to,
//! exactly or as a prefix written `<prefix>.*`, which the manifest's `[events]
//! subscribe` covers (`crate::registry`).
//!
//! **Held with its unit, and gone with it.** An event published where the call
//! tree holds a unit of work is the unit's: it reaches the queue when the unit
//! is kept, and a unit let go takes it with it; and a command that fails
//! having written, published, set or cancelled dooms the unit, so no caller
//! that goes on past the failure keeps the event. So a subscriber never hears
//! of a change that did not happen. One published where no unit is — a
//! command that only reads, a callback — is held by its call tree and queued
//! once the tree ends, and not at all when the journal refused the entry of
//! any of its dispatches: no subscriber hears of a read whose answer was
//! withheld (`crate::units::Later`).
//!
//! **Bounded before it is held.** The queue holds at most `event_queue` events
//! and `event_queue_bytes` of their topics and data, and the events a unit
//! holds for it are counted as already in it — so the commit that releases
//! them finds the room it was promised, no other call tree running between the
//! two. One past either bound is refused with `[NC-PLUG-0017]`, and nothing is
//! held.
//!
//! **Delivered once, in order, and decided again.** The pump takes the events
//! queued when it starts, oldest first, each once: the decision point decides
//! the authority its publisher ran under again, and each plugin that
//! subscribed hears it in a call tree of its own, run for the publisher's
//! principal, never confirmed, which only reads (`crate::pump`) — and so does
//! the callback of a timer set there (`crate::timers`).
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin announcing what another did,
//! a subscriber told of a change that was not kept, a queue that grows the
//! kernel without end, and a subscriber acting on an authority since
//! withdrawn. What it does not answer: what a subscriber does with what it
//! hears, which is its own code; an event published by a command that only
//! reads and then trapped, which is queued when its tree ends; and one plugin
//! publishing the queue full, which refuses every other plugin's events until
//! the next pump empties it — its two bounds are the kernel's, and no share of
//! them is a publisher's own, as a plugin's timers and surfaces are.

use std::collections::VecDeque;

use nc_authz::Principal;
use nc_catalog::{ErrorCode, KERNEL};

use crate::dispatch::{Authority, CallTree, Refusal, RefusalKind};
use crate::{Environment, PluginSlot, Shared};

/// How long a topic may be, in characters: as long as a command's name.
pub(crate) const TOPIC_MAX: usize = nc_schema::descriptor::NAME_MAX;

/// One event, as it waits for delivery.
///
/// Its topic and data, the plugin that published it, and who the publishing
/// call tree ran for under what authority — which its delivery runs for, and
/// decides again.
#[derive(Debug, Clone)]
pub(crate) struct Published {
    /// What it is about, in the publisher's namespace.
    pub(crate) topic: String,
    /// What it says, as canonical JSON.
    pub(crate) data: String,
    /// The plugin that published it.
    pub(crate) publisher: PluginSlot,
    /// Who the publishing call tree ran for.
    pub(crate) principal: Principal,
    /// What the publishing frame ran under.
    pub(crate) authority: Authority,
}

impl Published {
    /// How many bytes of the queue's bound it holds: its topic and its data.
    pub(crate) fn bytes(&self) -> u64 {
        u64::try_from(self.topic.len().saturating_add(self.data.len())).unwrap_or(u64::MAX)
    }
}

/// Whether `topic` is one: lower-case words joined by dots, at least two, at
/// most [`TOPIC_MAX`] characters.
pub(crate) fn topic_is_well_formed(topic: &str) -> bool {
    topic.len() <= TOPIC_MAX
        && topic.split('.').count() >= 2
        && topic.split('.').all(crate::manifest::a_word)
}

/// Whether `pattern`, as a manifest's `[events] subscribe` writes one, hears
/// a topic: the topic itself, or, written `<prefix>.*`, every longer one under it.
pub(crate) fn hears(pattern: &str, topic: &str) -> bool {
    pattern
        .strip_suffix(".*")
        .map_or(pattern == topic, |prefix| {
            topic
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.len() > 1 && rest.starts_with('.'))
        })
}

/// Refuses `topic` unless the plugin of `namespace` may publish under it.
///
/// # Errors
///
/// A refusal with `[NC-PLUG-0026]`: `invalid` for a topic that is not one,
/// and `denied` for one outside the namespace.
pub(crate) fn judged(topic: &str, namespace: &str) -> Result<(), Refusal> {
    if !topic_is_well_formed(topic) {
        return Err(Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::PluginTopicRefused,
            format!(
                "the topic {} is not lower-case words joined by dots, at least two and at most \
                 {TOPIC_MAX} characters",
                crate::registry::quoted_up_to(topic, TOPIC_MAX)
            ),
        ));
    }
    if !topic
        .strip_prefix(namespace)
        .is_some_and(|rest| rest.starts_with('.'))
    {
        return Err(Refusal::new(
            RefusalKind::Denied,
            ErrorCode::PluginTopicRefused,
            format!(
                "the topic `{topic}` is outside the namespace `{namespace}.` the plugin's \
                 manifest names, the one a plugin publishes in"
            ),
        ));
    }
    Ok(())
}

/// What an event says, as the canonical JSON it is kept and delivered as.
///
/// # Errors
///
/// `invalid`: with `[NC-VAL-0001]` past `payload_bytes`, and with the parser's
/// own code for a text that is not canonical JSON.
pub(crate) fn payload(data: &str) -> Result<String, Refusal> {
    if u64::try_from(data.len()).unwrap_or(u64::MAX) > KERNEL.payload_bytes {
        return Err(Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::ValidationOutOfRange,
            format!(
                "an event's data is {} bytes, past the {} a payload may be",
                data.len(),
                KERNEL.payload_bytes
            ),
        ));
    }
    nc_canon::parse(data)
        .map(|value| value.to_canonical())
        .map_err(|refused| {
            Refusal::new(
                RefusalKind::Invalid,
                refused.code(),
                format!("an event's data is not canonical JSON: {refused}"),
            )
        })
}

/// Publishes `event` from a frame of `tree`: held by the tree's unit when it
/// holds one, and by the tree until it ends when it does not.
///
/// # Errors
///
/// What [`Queue::room`] refuses, and then nothing is held.
pub(crate) fn publish<E: Environment>(
    shared: &Shared<E>,
    tree: &mut CallTree,
    event: Published,
) -> Result<(), Refusal> {
    crate::dispatch::lock(&shared.events).room(tree.held_events(), &event)?;
    tree.later_mut().publish(event);
    Ok(())
}

/// The events waiting for delivery, oldest first, and the bytes they hold.
#[derive(Debug, Default)]
pub(crate) struct Queue {
    waiting: VecDeque<Published>,
    bytes: u64,
}

impl Queue {
    /// Refuses `event` when the queue has no room for it beside `held`, the
    /// events a call tree or its unit holds for it.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` at `event_queue` events, or past the
    /// bytes `event_queue_bytes` allows their topics and data.
    pub(crate) fn room<'held>(
        &self,
        held: impl IntoIterator<Item = &'held Published>,
        event: &Published,
    ) -> Result<(), Refusal> {
        let (count, bytes) = held
            .into_iter()
            .fold((0_usize, self.bytes), |(count, bytes), held| {
                (count.saturating_add(1), bytes.saturating_add(held.bytes()))
            });
        let events = self.waiting.len().saturating_add(count);
        if u64::try_from(events).unwrap_or(u64::MAX) >= u64::from(KERNEL.event_queue) {
            return Err(Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::PluginBoundReached,
                format!(
                    "the events waiting for delivery, those units of work hold for the queue \
                     included, are at most {}, and this one would be one more",
                    KERNEL.event_queue
                ),
            ));
        }
        let bytes = bytes.saturating_add(event.bytes());
        if bytes > KERNEL.event_queue_bytes {
            return Err(Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::PluginBoundReached,
                format!(
                    "the events waiting for delivery hold at most {} bytes of topics and data, \
                     those units of work hold for the queue included, and this one of {} would \
                     take them to {bytes}",
                    KERNEL.event_queue_bytes,
                    event.bytes()
                ),
            ));
        }
        Ok(())
    }

    /// Queues `event`, last.
    pub(crate) fn push(&mut self, event: Published) {
        self.bytes = self.bytes.saturating_add(event.bytes());
        self.waiting.push_back(event);
    }

    /// How many events wait.
    pub(crate) fn len(&self) -> usize {
        self.waiting.len()
    }

    /// The oldest event, taken out of the queue: delivered once, whatever its
    /// delivery comes to.
    pub(crate) fn next(&mut self) -> Option<Published> {
        let event = self.waiting.pop_front()?;
        self.bytes = self.bytes.saturating_sub(event.bytes());
        Some(event)
    }

    /// Lets go of every event the plugin in `slot` published: one being
    /// unloaded, whose authority its events no longer run under.
    pub(crate) fn forget(&mut self, slot: PluginSlot) {
        self.waiting.retain(|event| event.publisher != slot);
        self.bytes = self.waiting.iter().map(Published::bytes).sum();
    }
}

#[cfg(test)]
mod tests {
    use nc_authz::EntityRef;
    use nc_catalog::{ErrorCode, KERNEL};

    use super::{Published, Queue, TOPIC_MAX, hears, judged, payload, topic_is_well_formed};
    use crate::PluginSlot;
    use crate::dispatch::{Authority, RefusalKind};

    /// The user `ana` of the world the policy cases are about.
    fn ana() -> nc_authz::Principal {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        nc_authz::MemoryDirectory::from_world(include_str!("../../../policy/world.json"), &pdp)
            .expect("the world loads")
            .principal(nc_authz::PrincipalKind::User, "ana")
            .expect("ana is a user")
    }

    /// An event of `data` under `topic`, published by the plugin in `slot`
    /// for `principal`.
    fn event(principal: &nc_authz::Principal, slot: usize, topic: &str, data: &str) -> Published {
        Published {
            topic: topic.to_owned(),
            data: data.to_owned(),
            publisher: PluginSlot(slot),
            principal: principal.clone(),
            authority: Authority {
                command: "reference.event.publish".to_owned(),
                action: "Write".to_owned(),
                resource: EntityRef {
                    type_name: "Content".to_owned(),
                    id: "photo".to_owned(),
                },
            },
        }
    }

    #[test]
    fn a_topic_is_two_words_or_more_each_lower_case_and_no_longer_than_a_name() {
        for topic in ["reference.saved", "reference.note.saved", "a.b-c", "a1.b2"] {
            assert!(topic_is_well_formed(topic), "{topic}");
        }
        for topic in [
            "reference",
            "Reference.saved",
            "reference.",
            ".saved",
            "reference..saved",
            "reference.saved-",
            "reference.1saved",
            "reference.sa ved",
            "reference.*",
        ] {
            assert!(!topic_is_well_formed(topic), "{topic}");
        }
        let longest = format!("a.{}", "b".repeat(TOPIC_MAX - 2));
        assert!(topic_is_well_formed(&longest));
        assert!(!topic_is_well_formed(&format!("{longest}b")));
    }

    #[test]
    fn a_pattern_hears_its_topic_and_a_prefix_hears_every_topic_below_it() {
        assert!(hears("reference.saved", "reference.saved"));
        assert!(!hears("reference.saved", "reference.saved.again"));
        assert!(!hears("reference.saved", "reference.save"));
        assert!(hears("reference.*", "reference.saved"));
        assert!(hears("reference.*", "reference.note.saved"));
        assert!(!hears("reference.*", "reference"));
        assert!(!hears("reference.*", "references.saved"));
        assert!(!hears("reference.note.*", "reference.note"));
        assert!(hears("reference.note.*", "reference.note.saved"));
    }

    #[test]
    fn a_plugin_publishes_only_well_formed_topics_of_its_own_namespace() {
        assert!(judged("reference.saved", "reference").is_ok());
        let outside = judged("peer.saved", "reference").expect_err("another's namespace");
        assert_eq!(
            (outside.kind(), outside.code()),
            (RefusalKind::Denied, ErrorCode::PluginTopicRefused)
        );
        let longer = judged("referenced.saved", "reference").expect_err("a longer namespace");
        assert_eq!(longer.kind(), RefusalKind::Denied);
        let malformed = judged("Reference.Saved", "reference").expect_err("not a topic");
        assert_eq!(
            (malformed.kind(), malformed.code()),
            (RefusalKind::Invalid, ErrorCode::PluginTopicRefused)
        );
        let long = judged(&"\u{1b}x".repeat(10_000), "reference").expect_err("not a topic");
        assert!(
            long.sentence().len() < 1_000 && !long.sentence().contains('\u{1b}'),
            "a topic that is not one is repeated cut and escaped: {}",
            long.sentence()
        );
    }

    #[test]
    fn an_events_data_is_a_payload_kept_as_its_canonical_text() {
        assert_eq!(
            payload(r#"{"b":1,"a":[true]}"#).expect("canonical JSON"),
            r#"{"a":[true],"b":1}"#
        );
        let float = payload(r#"{"a":1.5}"#).expect_err("a float is no payload");
        assert_eq!(float.kind(), RefusalKind::Invalid);
        assert_eq!(float.code(), ErrorCode::CanonNumber);
        let bound = usize::try_from(KERNEL.payload_bytes).expect("fits");
        let at_the_bound = format!("\"{}\"", "a".repeat(bound - 2));
        assert!(payload(&at_the_bound).is_ok(), "exactly a payload's bound");
        let past = payload(&format!("\"{}\"", "a".repeat(bound - 1))).expect_err("past it");
        assert_eq!(past.code(), ErrorCode::ValidationOutOfRange);
    }

    #[test]
    fn the_queue_holds_no_more_events_than_its_bound_counting_what_units_hold_for_it() {
        let ana = ana();
        let bound = usize::try_from(KERNEL.event_queue).expect("fits");
        let mut queue = Queue::default();
        for _ in 0..bound - 2 {
            let next = event(&ana, 0, "reference.saved", "{}");
            queue.room(&[], &next).expect("room");
            queue.push(next);
        }
        let held = vec![event(&ana, 0, "reference.saved", "{}")];
        let next = event(&ana, 0, "reference.saved", "{}");
        assert!(queue.room(&held, &next).is_ok(), "one short of the bound");
        let two_held = vec![held[0].clone(), held[0].clone()];
        let refused = queue.room(&two_held, &next).expect_err("at the bound");
        assert_eq!(
            (refused.kind(), refused.code()),
            (RefusalKind::Unavailable, ErrorCode::PluginBoundReached)
        );
        assert_eq!(queue.len(), bound - 2);
    }

    #[test]
    fn the_queue_holds_no_more_bytes_than_its_bound_counting_what_units_hold_for_it() {
        let ana = ana();
        let topic = "reference.saved";
        let data = format!("\"{}\"", "a".repeat(1_000_000 - 2));
        let one = event(&ana, 0, topic, &data).bytes();
        let fits = KERNEL.event_queue_bytes / one;
        let mut queue = Queue::default();
        for _ in 0..fits - 1 {
            let next = event(&ana, 0, topic, &data);
            queue.room(&[], &next).expect("room");
            queue.push(next);
        }
        let last = event(&ana, 0, topic, &data);
        assert!(queue.room(&[], &last).is_ok(), "the last that fits");
        let refused = queue
            .room(std::slice::from_ref(&last), &last)
            .expect_err("one held makes the next one too many bytes");
        assert_eq!(refused.code(), ErrorCode::PluginBoundReached);
        assert!(
            refused.sentence().contains("bytes"),
            "{}",
            refused.sentence()
        );
        assert_eq!(
            event(&ana, 0, "a.b", "{}").bytes(),
            5,
            "the topic and the data, counted"
        );
    }

    #[test]
    fn the_queue_gives_its_events_back_oldest_first_each_once() {
        let ana = ana();
        let mut queue = Queue::default();
        for (slot, topic) in [
            (0, "reference.one"),
            (1, "peer.two"),
            (0, "reference.three"),
        ] {
            queue.push(event(&ana, slot, topic, "{}"));
        }
        assert_eq!(
            queue.next().map(|event| event.topic).as_deref(),
            Some("reference.one")
        );
        queue.forget(PluginSlot(0));
        assert_eq!(queue.len(), 1, "the unloaded plugin's events are let go");
        assert_eq!(queue.bytes, event(&ana, 1, "peer.two", "{}").bytes());
        assert_eq!(
            queue.next().map(|event| event.topic).as_deref(),
            Some("peer.two")
        );
        assert!(queue.next().is_none());
        assert_eq!(queue.bytes, 0);
    }
}

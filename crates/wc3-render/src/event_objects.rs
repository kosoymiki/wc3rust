//! Event-object runtime (`EVTS`): turns the model's firing frames into
//! sound/effect triggers attached to a node.
//!
//! Each `EVTS` object is a node plus a list of frames
//! (`wc3_formats::mdx::EventTrack`). The node name decides the family:
//!
//! | prefix | family | action |
//! |--------|--------|--------|
//! | `SPN` | [`EventObjectKind::SpawnModel`] | spawn a model instance (footprint, gib, arrow trail) |
//! | `SPL` / `FPT` | [`EventObjectKind::Splat`] | draw a splat quad from `SplatData.txt` |
//! | `UBR` | [`EventObjectKind::UberSplat`] | draw an ubersplat quad |
//! | `SND` | [`EventObjectKind::Sound`] | play a positional sound |
//!
//! Triggering is **edge triggered**: an event fires when the sampler moves onto
//! a new firing frame, so a 30-frame hold does not retrigger every frame
//! (`mdx-m3-viewer/src/viewer/handlers/mdx/eventobjectemitterobject.ts:getValue`
//! returns the key index, and the handler compares it with the previous index).

use glam::{Mat4, Vec3};

use wc3_formats::mdx::{EventObject, EventObjectKind, MdxModel, Sequence};

use crate::attachments::AttachmentSet;

/// One fired event.
#[derive(Clone, Debug, PartialEq)]
pub struct EventTrigger {
    /// Event family from the node name prefix.
    pub kind: EventObjectKind,
    /// Table id from the node name (`"HBF0"` for `"SPNxHBF0"`).
    pub id: String,
    /// Node object id; the game parents spawned models to this node.
    pub node_id: u32,
    /// World-space position of the event node for this frame.
    pub position: Vec3,
    /// World matrix of the event node (spawn transform for `SPN`).
    pub matrix: Mat4,
}

/// Per-instance event state: the last sampled key index of each event object.
#[derive(Clone, Debug, Default)]
pub struct EventObjectRuntime {
    last_index: Vec<Option<usize>>,
}

impl EventObjectRuntime {
    /// Build the state for a model.
    pub fn new(model: &MdxModel) -> Self {
        Self {
            last_index: vec![None; model.event_objects.len()],
        }
    }

    /// Number of event objects tracked.
    pub fn len(&self) -> usize {
        self.last_index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.last_index.is_empty()
    }

    /// Forget the sampled indices (sequence change, respawn).
    pub fn reset(&mut self) {
        for slot in &mut self.last_index {
            *slot = None;
        }
    }

    /// Sample every event object for one frame and return the ones that fired.
    ///
    /// `frame_ms` is the sequence-relative time; `counter_ms` is the global
    /// animation clock used by events on a global sequence. Events outside the
    /// running sequence's `start..=end` do not fire, matching the donor's
    /// guard (`if (frame >= start && frame <= end)`).
    pub fn poll(
        &mut self,
        model: &MdxModel,
        sequence: Option<&Sequence>,
        frame_ms: u32,
        counter_ms: u32,
        node_worlds: &[Mat4],
        world: Mat4,
    ) -> Vec<EventTrigger> {
        let mut fired = Vec::new();
        for (index, event) in model.event_objects.iter().enumerate() {
            let Some(slot) = self.last_index.get_mut(index) else {
                continue;
            };
            let frame = match event.track.global_sequence {
                Some(period) if period > 0 => counter_ms % period,
                Some(_) => 0,
                None => match sequence {
                    Some(sequence) => {
                        if frame_ms < sequence.start || frame_ms > sequence.end {
                            *slot = None;
                            continue;
                        }
                        frame_ms
                    }
                    None => frame_ms,
                },
            };
            let sampled = event.track.index_at(frame);
            if sampled == *slot {
                continue;
            }
            *slot = sampled;
            if let Some(trigger) = trigger_for(event, node_worlds, world) {
                fired.push(trigger);
            }
        }
        fired
    }

    /// Convenience for the game layer: resolve a `SPN`/`SPL` id through the
    /// attachment sockets when the event node itself is not enough.
    pub fn position_via_attachment(
        event: &EventTrigger,
        attachments: &AttachmentSet,
        fallback: Vec3,
    ) -> Vec3 {
        if event.position.is_finite() && event.position != Vec3::ZERO {
            return event.position;
        }
        attachments
            .points()
            .iter()
            .find(|point| point.node_id == event.node_id)
            .map(|point| point.position())
            .unwrap_or(fallback)
    }
}

fn trigger_for(event: &EventObject, node_worlds: &[Mat4], world: Mat4) -> Option<EventTrigger> {
    let (kind, id) = event.kind_and_id();
    if kind == EventObjectKind::Unknown {
        return None;
    }
    let local = node_worlds
        .get(event.node.object_id as usize)
        .copied()
        .unwrap_or(Mat4::IDENTITY);
    let matrix = world * local;
    Some(EventTrigger {
        kind,
        id,
        node_id: event.node.object_id,
        position: matrix.transform_point3(Vec3::ZERO),
        matrix,
    })
}

/// Split fired triggers by family, ready for the effect/audio layers.
#[derive(Default, Debug, PartialEq)]
pub struct EventBatch {
    pub sounds: Vec<EventTrigger>,
    pub splats: Vec<EventTrigger>,
    pub ubersplats: Vec<EventTrigger>,
    pub spawns: Vec<EventTrigger>,
}

impl EventBatch {
    pub fn from_triggers(triggers: Vec<EventTrigger>) -> Self {
        let mut batch = Self::default();
        for trigger in triggers {
            match trigger.kind {
                EventObjectKind::Sound => batch.sounds.push(trigger),
                EventObjectKind::Splat => batch.splats.push(trigger),
                EventObjectKind::UberSplat => batch.ubersplats.push(trigger),
                EventObjectKind::SpawnModel => batch.spawns.push(trigger),
                EventObjectKind::Unknown => {}
            }
        }
        batch
    }

    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
            && self.splats.is_empty()
            && self.ubersplats.is_empty()
            && self.spawns.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sounds.len() + self.splats.len() + self.ubersplats.len() + self.spawns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use wc3_formats::mdx::{EventObject, EventTrack, Interpolation, Node, Track};

    fn node(name: &str, object_id: u32) -> Node {
        let empty = || Track::<Vec3> {
            interpolation: Interpolation::None,
            global_sequence: None,
            keys: Vec::new(),
            static_value: None,
        };
        Node {
            name: name.to_string(),
            object_id,
            parent_id: -1,
            flags: 0,
            translation: empty(),
            rotation: Track {
                interpolation: Interpolation::None,
                global_sequence: None,
                keys: Vec::new(),
                static_value: None,
            },
            scaling: empty(),
        }
    }

    fn model() -> MdxModel {
        let mut model = MdxModel::default();
        model.sequences.push(Sequence {
            name: "Stand".to_string(),
            start: 0,
            end: 200,
            move_speed: 0.0,
            flags: 0,
            rarity: 0.0,
            sync_point: 0,
            bounds: wc3_formats::mdx::Extent::default(),
        });
        model.event_objects.push(EventObject {
            node: node("SNDxHELLO", 0),
            track: EventTrack {
                frames: vec![100],
                global_sequence: None,
            },
        });
        model.event_objects.push(EventObject {
            node: node("SPNxFOOT", 1),
            track: EventTrack {
                frames: vec![50, 150],
                global_sequence: None,
            },
        });
        model.event_objects.push(EventObject {
            node: node("SPLxBASE", 2),
            track: EventTrack {
                frames: vec![10],
                global_sequence: Some(1000),
            },
        });
        model
    }

    #[test]
    fn fires_once_per_frame_and_only_inside_the_sequence() {
        let model = model();
        let mut runtime = EventObjectRuntime::new(&model);
        let sequence = model.sequences[0].clone();
        let identity = [Mat4::IDENTITY; 4];
        let mut poll = |frame: u32| {
            runtime.poll(&model, Some(&sequence), frame, 0, &identity, Mat4::IDENTITY)
        };

        // Playback starts at frame 0: nothing has fired yet.
        assert!(poll(0).is_empty(), "frame 0 fires nothing");
        assert!(poll(49).is_empty(), "frame 49 fires nothing");
        // The spawn event's first firing frame is 50.
        let first = poll(50);
        assert_eq!(first.len(), 1, "spawn at frame 50: {first:?}");
        assert_eq!(first[0].kind, EventObjectKind::SpawnModel);
        assert_eq!(first[0].id, "FOOT");
        assert_eq!(first[0].node_id, 1);
        // Edge triggered: the following frames stay quiet.
        assert!(poll(51).is_empty(), "no repeat after the firing frame");
        // The sound fires at 100, once.
        let sound = poll(100);
        assert_eq!(sound.len(), 1, "sound at frame 100: {sound:?}");
        assert_eq!(sound[0].kind, EventObjectKind::Sound);
        assert_eq!(sound[0].id, "HELLO");
        assert!(poll(101).is_empty(), "no repeat after the sound frame");
        // The spawn event fires again at 150.
        let second = poll(150);
        assert_eq!(second.len(), 1, "{second:?}");
        assert_eq!(second[0].kind, EventObjectKind::SpawnModel);
        // A frame outside the sequence is ignored even when a key is present.
        let outside = poll(250);
        assert!(outside.is_empty(), "{outside:?}");
    }

    #[test]
    fn global_sequence_events_use_the_counter() {
        let model = model();
        let mut runtime = EventObjectRuntime::new(&model);
        let identity = [Mat4::IDENTITY; 4];
        let fired = runtime.poll(&model, None, 0, 10, &identity, Mat4::IDENTITY);
        assert_eq!(fired.len(), 1, "{fired:?}");
        assert_eq!(fired[0].kind, EventObjectKind::Splat);
        assert_eq!(fired[0].id, "BASE");
        // 10 again (one period later) is the same key index: still no re-fire.
        let refire = runtime.poll(&model, None, 0, 1010, &identity, Mat4::IDENTITY);
        assert!(refire.is_empty(), "{refire:?}");
        // Rewinding resets the sampled index and the event fires again.
        runtime.reset();
        let again = runtime.poll(&model, None, 0, 10, &identity, Mat4::IDENTITY);
        assert_eq!(again.len(), 1);
    }

    #[test]
    fn trigger_position_follows_the_node_matrix() {
        let model = model();
        let mut runtime = EventObjectRuntime::new(&model);
        let mut worlds = [Mat4::IDENTITY; 4];
        worlds[0] = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let world = Mat4::from_translation(Vec3::new(0.0, 10.0, 0.0));
        // Warm the state up to the sound frame so only that event is returned.
        let _ = runtime.poll(&model, None, 0, 0, &worlds, world);
        let _ = runtime.poll(&model, None, 50, 0, &worlds, world);
        let fired = runtime.poll(&model, None, 100, 0, &worlds, world);
        assert_eq!(fired.len(), 1, "{fired:?}");
        assert_eq!(fired[0].kind, EventObjectKind::Sound);
        assert_eq!(fired[0].position, Vec3::new(1.0, 12.0, 3.0));
    }

    #[test]
    fn batches_split_by_family() {
        let model = model();
        let mut runtime = EventObjectRuntime::new(&model);
        let identity = [Mat4::IDENTITY; 4];
        let mut batch = EventBatch::default();
        for frame in [0u32, 50, 100, 150] {
            let fired = runtime.poll(&model, None, frame, 0, &identity, Mat4::IDENTITY);
            let step = EventBatch::from_triggers(fired);
            batch.sounds.extend(step.sounds);
            batch.splats.extend(step.splats);
            batch.ubersplats.extend(step.ubersplats);
            batch.spawns.extend(step.spawns);
        }
        assert_eq!(batch.spawns.len(), 2, "frames 50 and 150");
        assert_eq!(batch.sounds.len(), 1, "frame 100");
        assert_eq!(batch.len(), 3);
        assert!(!batch.is_empty());
    }

    #[test]
    fn real_model_event_frames_are_reachable() {
        // FlareTarget fires its sound at frame 33; the fixture test proves the
        // parser and the runtime agree on the same frame list.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut path = None;
        for tree in ["analysis/war3_samples", "analysis/war3_full"] {
            let candidate = root
                .join(tree)
                .join("Abilities/Spells/Human/Flare/FlareTarget.mdx");
            if candidate.exists() {
                path = Some(candidate);
                break;
            }
        }
        let Some(path) = path else {
            eprintln!("skipping: FlareTarget.mdx fixture missing");
            return;
        };
        let bytes = std::fs::read(path).expect("read fixture");
        let model = wc3_formats::mdx::MdxModel::parse(&bytes).expect("parse");
        let Some(event) = model.event_objects.first() else {
            eprintln!("skipping: FlareTarget has no event objects");
            return;
        };
        assert_eq!(event.track.frames.first().copied(), Some(33));
        let mut runtime = EventObjectRuntime::new(&model);
        let identity = vec![Mat4::IDENTITY; model.pivots.len().max(64)];
        let before = runtime.poll(&model, None, 32, 0, &identity, Mat4::IDENTITY);
        assert!(before.is_empty(), "{before:?}");
        let at = runtime.poll(&model, None, 33, 0, &identity, Mat4::IDENTITY);
        assert_eq!(at.len(), 1);
        assert_eq!(at[0].kind, EventObjectKind::Sound);
    }
}

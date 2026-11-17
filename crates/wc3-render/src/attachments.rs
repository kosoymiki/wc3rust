//! Attachment points (`ATCH`): the model-space sockets a weapon, shield, cap or
//! effect is parented to.
//!
//! MDX stores one `ATCH` object per socket: a generic node (with its own
//! animation tracks), a 260-byte `path` such as `"weapon"` or `"overhead ref"`
//! and a `attachment_id`. The node is a regular node object, so the animated
//! stack (`wc3_anim`) already produces its world matrix; this module turns those
//! matrices into named, world-space sockets and classifies them into the slots
//! the game layer asks for.
//!
//! # Real-data note
//!
//! War3's shipped models are inconsistent: the Footman has nine attachments
//! whose `path` is **empty** and whose meaning lives in the node name
//! (`"Weapon Ref"`, `"Hand Right Ref"`, `"OverHead Ref"`, `"Origin Ref"`), while
//! other models fill the path (`"weapon"`, `"overhead"`). Lookup therefore
//! accepts either and normalises case and separators.

use glam::{Mat4, Vec3};
use wc3_formats::mdx::MdxModel;

/// Semantic socket slot, derived from the path or the node name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttachmentSlot {
    Weapon,
    HandLeft,
    HandRight,
    Chest,
    Head,
    Overhead,
    Origin,
    FootLeft,
    FootRight,
    Other,
}

impl AttachmentSlot {
    /// Classify a raw `path`/`name` string.
    ///
    /// Words are compared after lowercasing and replacing `.`, `_` and `-` with
    /// spaces, so `"Hand.Right"`, `"hand_right"` and `"Hand Right Ref"` all
    /// resolve to [`AttachmentSlot::HandRight`].
    pub fn classify(raw: &str) -> Self {
        let text = normalise(raw);
        // Order matters: "hand right" contains "head"? No, but "overhead"
        // contains "head", so the longer, more specific slots come first.
        if text.contains("weapon") {
            return Self::Weapon;
        }
        if text.contains("overhead") || text.contains("over head") {
            return Self::Overhead;
        }
        if text.contains("hand right") || text.contains("right hand") || text.contains("hand r") {
            return Self::HandRight;
        }
        if text.contains("hand left") || text.contains("left hand") || text.contains("hand l") {
            return Self::HandLeft;
        }
        if text.contains("foot left") || text.contains("left foot") {
            return Self::FootLeft;
        }
        if text.contains("foot right") || text.contains("right foot") {
            return Self::FootRight;
        }
        if text.contains("chest") || text.contains("torso") {
            return Self::Chest;
        }
        if text.contains("head") || text.contains("mount") {
            return Self::Head;
        }
        if text.contains("origin") || text.contains("root") || text.contains("base") {
            return Self::Origin;
        }
        Self::Other
    }

    /// Stable name for logs and tests.
    pub fn name(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::HandLeft => "hand_left",
            Self::HandRight => "hand_right",
            Self::Chest => "chest",
            Self::Head => "head",
            Self::Overhead => "overhead",
            Self::Origin => "origin",
            Self::FootLeft => "foot_left",
            Self::FootRight => "foot_right",
            Self::Other => "other",
        }
    }
}

fn normalise(raw: &str) -> String {
    let lowered = raw.to_ascii_lowercase();
    let mut out = String::with_capacity(lowered.len());
    for ch in lowered.chars() {
        match ch {
            '.' | '_' | '-' | '\\' | '/' => out.push(' '),
            other => out.push(other),
        }
    }
    // Collapse double spaces so `contains("hand  right")` never misses.
    let mut collapsed = String::with_capacity(out.len());
    let mut previous_space = false;
    for ch in out.chars() {
        if ch == ' ' {
            if !previous_space {
                collapsed.push(' ');
            }
            previous_space = true;
        } else {
            collapsed.push(ch);
            previous_space = false;
        }
    }
    collapsed.trim().to_string()
}

/// One resolved attachment socket.
#[derive(Clone, Debug)]
pub struct AttachmentPoint {
    /// `ATCH` node name (`"Weapon Ref"`).
    pub name: String,
    /// Raw `ATCH` path (`"weapon"`, often empty in shipped models).
    pub path: String,
    /// Node object id, the index into the model's world matrices.
    pub node_id: u32,
    /// Semantic slot.
    pub slot: AttachmentSlot,
    /// Model-space pivot of the node (the socket's anchor).
    pub pivot: Vec3,
    /// Model-space matrix of the socket (the node's animated world matrix).
    ///
    /// The animated stack composes node matrices as "rotation/scaling about the
    /// pivot" (`wc3_anim::node_local_matrix` = `T(p) * pose * T(-p)`), so this
    /// matrix maps node-local space into model space; the socket *position* is
    /// this matrix applied to [`Self::pivot`], not to the origin.
    pub local: Mat4,
    /// World matrix of the socket (`world * local`).
    pub matrix: Mat4,
}

impl AttachmentPoint {
    /// World-space position of the socket.
    pub fn position(&self) -> Vec3 {
        self.matrix.transform_point3(self.pivot)
    }

    /// World matrix for a child model attached to this socket: the child's
    /// origin lands on the socket and its axes follow the node.
    pub fn attach(&self, child_local: Mat4) -> Mat4 {
        self.matrix * Mat4::from_translation(self.pivot) * child_local
    }
}

/// All attachments of one model instance for one frame.
#[derive(Clone, Debug, Default)]
pub struct AttachmentSet {
    points: Vec<AttachmentPoint>,
}

impl AttachmentSet {
    /// Resolve every `ATCH` of `model` against the animated node matrices.
    ///
    /// `node_worlds` is indexed by node object id (the layout
    /// `wc3_anim::Animator::world_matrices` produces), `world` is the model
    /// instance transform.
    pub fn from_model(model: &MdxModel, node_worlds: &[Mat4], world: Mat4) -> Self {
        let mut points = Vec::with_capacity(model.attachments.len());
        for attachment in &model.attachments {
            let node_id = attachment.node.object_id;
            let local = node_worlds
                .get(node_id as usize)
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            let pivot = model
                .pivots
                .get(node_id as usize)
                .copied()
                .unwrap_or(Vec3::ZERO);
            // The node name is the fallback meaning when `path` is empty, and
            // also disambiguates two sockets that share a path.
            let slot = {
                let from_path = AttachmentSlot::classify(&attachment.path);
                if from_path == AttachmentSlot::Other {
                    AttachmentSlot::classify(&attachment.node.name)
                } else {
                    from_path
                }
            };
            points.push(AttachmentPoint {
                name: attachment.node.name.clone(),
                path: attachment.path.clone(),
                node_id,
                slot,
                pivot,
                local,
                matrix: world * local,
            });
        }
        Self { points }
    }

    /// Build a set from already-resolved points (tests, custom sockets).
    pub fn from_points(points: Vec<AttachmentPoint>) -> Self {
        Self { points }
    }

    pub fn points(&self) -> &[AttachmentPoint] {
        &self.points
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// First socket whose `path` or `name` contains `needle` (case and
    /// separator insensitive).
    pub fn find(&self, needle: &str) -> Option<&AttachmentPoint> {
        let needle = normalise(needle);
        if needle.is_empty() {
            return None;
        }
        self.points
            .iter()
            .find(|point| {
                normalise(&point.path).contains(&needle) || normalise(&point.name).contains(&needle)
            })
    }

    /// First socket of a slot.
    pub fn slot(&self, slot: AttachmentSlot) -> Option<&AttachmentPoint> {
        self.points.iter().find(|point| point.slot == slot)
    }

    /// Weapon socket (right hand by convention).
    pub fn weapon(&self) -> Option<&AttachmentPoint> {
        self.slot(AttachmentSlot::Weapon)
    }

    /// Right hand socket, falling back to the weapon socket when a model only
    /// exposes the weapon.
    pub fn hand_right(&self) -> Option<&AttachmentPoint> {
        self.slot(AttachmentSlot::HandRight)
            .or_else(|| self.slot(AttachmentSlot::Weapon))
    }

    pub fn hand_left(&self) -> Option<&AttachmentPoint> {
        self.slot(AttachmentSlot::HandLeft)
    }

    pub fn overhead(&self) -> Option<&AttachmentPoint> {
        self.slot(AttachmentSlot::Overhead)
    }

    pub fn origin(&self) -> Option<&AttachmentPoint> {
        self.slot(AttachmentSlot::Origin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;
    use wc3_formats::mdx::{Attachment, Interpolation, Node, Track};

    fn empty_track<T: Copy>() -> Track<T> {
        Track {
            interpolation: Interpolation::None,
            global_sequence: None,
            keys: Vec::new(),
            static_value: None,
        }
    }

    fn node(name: &str, object_id: u32) -> Node {
        Node {
            name: name.to_string(),
            object_id,
            parent_id: -1,
            flags: 0,
            translation: empty_track(),
            rotation: empty_track(),
            scaling: empty_track(),
        }
    }

    fn model_with(names: &[(&str, &str)]) -> MdxModel {
        let mut model = MdxModel::default();
        for (index, (name, path)) in names.iter().enumerate() {
            model.attachments.push(Attachment {
                node: node(name, index as u32),
                path: path.to_string(),
                attachment_id: 0,
            });
        }
        model
    }

    #[test]
    fn classifies_donor_socket_names() {
        assert_eq!(AttachmentSlot::classify("weapon"), AttachmentSlot::Weapon);
        assert_eq!(AttachmentSlot::classify("Weapon Ref"), AttachmentSlot::Weapon);
        assert_eq!(
            AttachmentSlot::classify("Hand Right Ref"),
            AttachmentSlot::HandRight
        );
        assert_eq!(
            AttachmentSlot::classify("Hand.Left"),
            AttachmentSlot::HandLeft
        );
        assert_eq!(
            AttachmentSlot::classify("OverHead Ref"),
            AttachmentSlot::Overhead
        );
        assert_eq!(AttachmentSlot::classify("Origin Ref"), AttachmentSlot::Origin);
        assert_eq!(AttachmentSlot::classify("Chest Ref"), AttachmentSlot::Chest);
        assert_eq!(
            AttachmentSlot::classify("Foot Left Ref"),
            AttachmentSlot::FootLeft
        );
        assert_eq!(AttachmentSlot::classify("sprite first"), AttachmentSlot::Other);
    }

    #[test]
    fn world_matrix_is_model_transform_times_node() {
        let model = model_with(&[("Weapon Ref", "")]);
        // Node 0 sits at (10, 0, 0) with a 90 degree Z rotation.
        let local = Mat4::from_rotation_translation(
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::new(10.0, 0.0, 0.0),
        );
        let world = Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0));
        let set = AttachmentSet::from_model(&model, &[local], world);
        assert_eq!(set.len(), 1);
        let weapon = set.weapon().expect("weapon socket");
        assert_eq!(weapon.name, "Weapon Ref");
        assert_eq!(weapon.node_id, 0);
        let position = weapon.position();
        assert!((position - Vec3::new(10.0, 0.0, 5.0)).length() < 1e-5, "{position:?}");
        // A child 1 unit along +X of the socket lands on +Y in world (the
        // socket's rotation is applied before the model transform).
        let child = weapon.attach(Mat4::from_translation(Vec3::X));
        let child_position = child.transform_point3(Vec3::ZERO);
        assert!((child_position - Vec3::new(10.0, 1.0, 5.0)).length() < 1e-5, "{child_position:?}");
    }

    #[test]
    fn lookup_accepts_path_or_name_and_falls_back_to_weapon() {
        let model = model_with(&[
            ("Hand Right Ref", ""),
            ("Chest Ref", ""),
            ("Origin Ref", ""),
        ]);
        let identity = [Mat4::IDENTITY; 3];
        let set = AttachmentSet::from_model(&model, &identity, Mat4::IDENTITY);
        assert!(set.find("hand right").is_some());
        assert!(set.find("HAND.RIGHT").is_some());
        assert!(set.find("chest").is_some());
        assert!(set.find("origin").is_some());
        assert!(set.find("weapon").is_none());
        // `hand_right` resolves directly; there is no weapon socket to fall
        // back to, so `weapon()` is `None` while `hand_right()` is present.
        assert!(set.hand_right().is_some());
        assert!(set.weapon().is_none());
    }

    #[test]
    fn path_wins_over_name_when_both_are_present() {
        let mut model = MdxModel::default();
        model.attachments.push(Attachment {
            node: node("Custom Socket", 0),
            path: "weapon".to_string(),
            attachment_id: 0,
        });
        let set = AttachmentSet::from_model(&model, &[Mat4::IDENTITY], Mat4::IDENTITY);
        assert_eq!(set.weapon().expect("weapon").name, "Custom Socket");
    }

    #[test]
    fn real_footman_attachment_sockets_resolve() {
        // War3.mpq Footman: nine ATCH objects, empty paths, names carry the
        // meaning. Skips when the extracted corpus is absent.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut path = None;
        for tree in ["analysis/war3_samples", "analysis/war3_full"] {
            let candidate = root.join(tree).join("Units/Human/Footman/Footman.mdx");
            if candidate.exists() {
                path = Some(candidate);
                break;
            }
        }
        let Some(path) = path else {
            eprintln!("skipping: Footman.mdx fixture missing");
            return;
        };
        let bytes = std::fs::read(path).expect("read fixture");
        let model = wc3_formats::mdx::MdxModel::parse(&bytes).expect("parse");
        assert_eq!(model.attachments.len(), 9);
        let worlds = vec![Mat4::IDENTITY; model.pivots.len().max(64)];
        let set = AttachmentSet::from_model(&model, &worlds, Mat4::IDENTITY);
        assert_eq!(set.len(), 9);
        for slot in [
            AttachmentSlot::Weapon,
            AttachmentSlot::HandRight,
            AttachmentSlot::HandLeft,
            AttachmentSlot::Overhead,
            AttachmentSlot::Origin,
            AttachmentSlot::Chest,
            AttachmentSlot::Head,
            AttachmentSlot::FootLeft,
            AttachmentSlot::FootRight,
        ] {
            assert!(set.slot(slot).is_some(), "missing slot {}", slot.name());
        }
        assert!(set.weapon().is_some());
        assert!(set.find("hand right").is_some());
        assert!(set.find("overhead").is_some());
    }
}

//! What the user has picked in the rig editor. Shared by the canvas, the 3D
//! view, the results list and the inspector.

use bth_rigging::rig::preset::{BodyPreset, HardwarePreset};
use bth_rigging::rig::topo::TopoEdit;
use bth_rigging::rig::{BodyId, BodyKind, MemberId, NodeId, Rig};

/// A palette item waiting for a place on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Armed {
    Body(BodyPreset),
    MasterLink,
    Hardware(HardwarePreset),
}

impl Armed {
    pub fn label(self) -> &'static str {
        match self {
            Self::Body(preset) => preset.label(),
            Self::MasterLink => "Master link (junction)",
            Self::Hardware(preset) => preset.label(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Body(BodyId),
    /// A port on a body, or a master link.
    Node(NodeId),
    Member(MemberId),
    Component {
        member: MemberId,
        segment: usize,
        index: usize,
    },
}

impl Selection {
    /// The member this selection belongs to, if any.
    pub fn member(self) -> Option<MemberId> {
        match self {
            Self::Member(id) | Self::Component { member: id, .. } => Some(id),
            _ => None,
        }
    }

    /// The edit that deletes this item; `None` for the hook.
    pub fn delete_edit(self, rig: &Rig) -> Option<TopoEdit> {
        match self {
            Self::Body(id) => {
                let hook = matches!(rig.bodies.get(&id)?.kind, BodyKind::Hook);
                (!hook).then_some(TopoEdit::RemoveBody { id })
            }
            Self::Node(id) => {
                let root = rig.nodes.get(&id)?.kind.is_root();
                (!root).then_some(TopoEdit::RemoveNode { id })
            }
            Self::Member(id) => Some(TopoEdit::RemoveMember { id }),
            Self::Component {
                member,
                segment,
                index,
            } => Some(TopoEdit::RemoveComponent {
                member,
                segment,
                index,
            }),
        }
    }

    /// Whether the selected item still exists after an edit.
    pub fn exists_in(self, rig: &Rig) -> bool {
        match self {
            Self::Body(id) => rig.bodies.contains_key(&id),
            Self::Node(id) => rig.nodes.contains_key(&id),
            Self::Member(id) => rig.members.contains_key(&id),
            Self::Component {
                member,
                segment,
                index,
            } => rig
                .members
                .get(&member)
                .and_then(|m| m.segments.get(segment))
                .is_some_and(|s| index < s.components.len()),
        }
    }
}

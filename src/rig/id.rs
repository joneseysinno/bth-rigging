//! Typed identifiers for rig graph elements.
//!
//! Step: 1
//! Theory: roadmap Step 1 — UUID keys that survive reordering and merges.
//! Inputs: new v4 UUIDs, or values loaded from the store.
//! Outputs: `BodyId`, `NodeId`, `MemberId`, `ParamId`.
//! Must not depend on: solve, UI, dioxus.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident, $label:expr) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn nil() -> Self {
                Self(Uuid::nil())
            }

            pub fn is_nil(self) -> bool {
                self.0.is_nil()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}:{}", $label, self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }
    };
}

typed_id!(BodyId, "body");
typed_id!(NodeId, "node");
typed_id!(MemberId, "member");
typed_id!(ParamId, "param");

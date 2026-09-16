//! Hardware catalogs: screw-pin shackles and roundsling ratings.
//!
//! Depends only on `domain`. Roadmap: Step 0 foundation; connection-hardware table in Step 1/3.

pub mod roundsling;
pub mod shackle;

pub use roundsling::{find_by_size, RoundSlingRating, POLYESTER_ROUNDSLINGS};
pub use shackle::{find_shackle, ShackleRating, SHACKLES};

pub mod connection_hardware;


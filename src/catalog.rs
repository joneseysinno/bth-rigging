//! Hardware catalogs: screw-pin shackles and roundsling ratings.
//!
//! Depends only on `domain`. Roadmap: Step 0 foundation; connection-hardware table in Step 1/3.

pub mod roundsling;
pub mod shackle;

pub use roundsling::{POLYESTER_ROUNDSLINGS, RoundSlingRating, find_by_size};
pub use shackle::{SHACKLES, ShackleRating, find_shackle};

pub mod connection_hardware;

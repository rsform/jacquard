//! CAR (Content Addressable aRchive) file I/O
//!
//! Provides utilities for reading and writing CAR files, which are the standard
//! format for AT Protocol repository export/import.
//!
//! # Examples
//!
//! Reading a CAR file:
//! ```ignore
//! use jacquard_repo::car::reader::read_car;
//!
//! let blocks = read_car("repo.car").await?;
//! ```
//!
//! Writing a CAR file:
//! ```ignore
//! use jacquard_repo::car::writer::write_car;
//!
//! let roots = vec![commit_cid];
//! write_car("repo.car", roots, blocks).await?;
//! ```

pub mod reader;
pub mod writer;

// Re-export commonly used functions and types
pub use reader::{parse_car_bytes, read_car, read_car_header, stream_car, ParsedCar};
pub use writer::{export_repo_car, write_car, write_car_bytes};

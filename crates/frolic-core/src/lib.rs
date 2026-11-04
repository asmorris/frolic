//! # frolic-core
//!
//! Physics core of Frolic, the magnetic field simulator, independent of any rendering engine.
//!
//! * [`FieldSource`]: the extension trait for anything that produces a B-field.
//! * [`Magnet`]: uniformly magnetized permanent magnets built on exact analytic
//!   solutions for [`Shape`] primitives (block, cylinder, sphere).
//! * [`FieldScene`]: an immutable snapshot of sources; the field is their superposition.
//! * [`fieldlines`]: flux-weighted seeding and RK4 field-line tracing.
//!
//! Units: B in tesla, magnetization given as polarization `J = μ0·M` in tesla, lengths in
//! any consistent unit (the app uses centimetres).

pub mod analytic;
pub mod fieldlines;
pub mod magnet;
pub mod scene;
pub mod shape;
pub mod source;
pub mod special;

pub use fieldlines::{trace_field_lines, FieldLine, TraceParams};
pub use glam::{DQuat, DVec3};
pub use magnet::{Magnet, NDFEB_N42_BR};
pub use scene::FieldScene;
pub use shape::{Shape, SurfaceSample};
pub use source::{FieldSource, Pose};

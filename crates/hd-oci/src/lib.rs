//! # hd-oci
//!
//! OCI image ingestion and Dockerfile translation for hyperdocker.
//!
//! This crate handles interoperability with the OCI container ecosystem. It can
//! pull OCI images from container registries, unpack their layers into the CAS,
//! and translate Dockerfiles into `hd.toml` environment specs for a smooth
//! migration path from Docker to hyperdocker.
//!
//! ## Key Types
//!
//! - [`ImageRef`] - Reference to an OCI image (registry/repo:tag)
//! - [`unpack_layer`] - Unpacks a tar+gzip OCI layer into the CAS
//! - [`UnpackedEntry`] - A single file extracted from a layer
//! - [`translate_dockerfile`] - Converts a Dockerfile into an `hd.toml` spec
//!
//! ## Example
//!
//! ```no_run
//! use hd_oci::{ImageRef, translate_dockerfile};
//!
//! // Translate Dockerfile content into an EnvSpec
//! let content = std::fs::read_to_string("Dockerfile").unwrap();
//! let spec = translate_dockerfile(&content).unwrap();
//! println!("{:?}", spec);
//!
//! // Reference an OCI image
//! let image = ImageRef::parse("ubuntu:22.04").unwrap();
//! println!("pulling {}", image);
//! ```

pub mod registry;
pub mod unpack;
pub mod dockerfile;

pub use registry::{ImageRef, RegistryError};
pub use unpack::{unpack_layer, UnpackedEntry, UnpackError};
pub use dockerfile::{translate_dockerfile, DockerfileError};

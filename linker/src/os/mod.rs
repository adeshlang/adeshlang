//! Operating system targets and platform conventions.

pub mod freebsd;
pub mod linux;
pub mod macos;
pub mod none;
pub mod openbsd;
pub mod windows;

pub use freebsd::FreeBsdOs;
pub use linux::LinuxOs;
pub use macos::MacOs;
pub use none::NoneOs;
pub use openbsd::{
    AixOs, AndroidOs, DragonFlyOs, IosOs, NetBsdOs, OpenBsdOs, Plan9Os, QuantumRuntimeOs, SolarisOs,
    WasiOs,
};
pub use windows::WindowsOs;

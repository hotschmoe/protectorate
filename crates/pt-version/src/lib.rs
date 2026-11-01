//! The release stamp every binary prints and reports: `PROTECTORATE_BUILD_VERSION` when the build
//! sets it (the Makefile and release.sh do), else `git describe` of the checkout at build time,
//! else `dev`.

#![forbid(unsafe_code)]

pub const VERSION: &str = env!("PT_BUILD_VERSION");

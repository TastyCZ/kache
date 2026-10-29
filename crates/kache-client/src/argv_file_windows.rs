//! Windows' side of the forwarded argv file: arguments as UTF-16 code units,
//! and the error that sends a command line to a file.

use anyhow::{Context, Result};
use std::ffi::{OsStr, OsString};

/// Windows' `ERROR_FILENAME_EXCED_RANGE`: a command line past 32,767
/// characters, Cargo's own test for writing an argfile.
pub(crate) fn command_line_too_long(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(206)
}

pub(super) fn encode<'a>(args: impl IntoIterator<Item = &'a OsStr>) -> Result<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;
    let mut bytes = Vec::new();
    for arg in args {
        for unit in arg.encode_wide() {
            anyhow::ensure!(unit != 0, "an argument holds a NUL");
            bytes.extend(unit.to_le_bytes());
        }
        bytes.extend(0u16.to_le_bytes());
    }
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<Vec<OsString>> {
    use std::os::windows::ffi::OsStringExt;
    let (pairs, odd) = bytes.as_chunks::<2>();
    anyhow::ensure!(odd.is_empty(), "odd length");
    let units = pairs
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect::<Vec<_>>();
    let units = units.strip_suffix(&[0]).context("unterminated argument")?;
    Ok(units
        .split(|unit| *unit == 0)
        .map(OsString::from_wide)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_command_line_too_long_counts_as_one() {
        assert!(command_line_too_long(&std::io::Error::from_raw_os_error(
            206
        )));
        assert!(!command_line_too_long(&std::io::Error::from_raw_os_error(
            2
        )));
        assert!(!command_line_too_long(&std::io::Error::other("206")));
    }
}

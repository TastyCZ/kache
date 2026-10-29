//! Start a cold daemon while retaining compiler creation in the frontend.

use crate::Execution;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

pub(super) fn try_bootstrap(
    execution: &Execution<'_>,
    socket: &Path,
    disabled: bool,
    trusted_pipeline: bool,
) -> Result<Option<i32>> {
    let bootstrap = ["kache".into(), "daemon".into(), "start".into()];
    if !disabled
        && let Ok(mut command) = super::engine_command(
            execution.executable,
            execution.client_version,
            &bootstrap,
            execution.started_at,
        )
        && command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    {
        execution.try_execute_with_trust(socket, trusted_pipeline)
    } else {
        Ok(None)
    }
}

/// Start the engine. A command line Windows refuses starts again with its
/// arguments in a file, which stays until the engine has exited: the engine
/// reads it whenever it asks for its arguments.
pub(super) fn engine_status(command: &mut Command) -> Result<ExitStatus> {
    match command.status() {
        Err(error) if crate::argv_file::command_line_too_long(&error) => {
            let (mut short, _file) = through_argv_file(command, &std::env::temp_dir())?;
            short
                .status()
                .context("starting the Kache engine with its arguments in a file")
        }
        status => status.context("starting the Kache engine"),
    }
}

/// `command` with its arguments in a file. A compiler command line just short
/// of Windows' limit fits Cargo's start of the frontend, so Cargo wrote no
/// argfile, but not the engine's, which adds the envelope.
fn through_argv_file(
    command: &Command,
    directory: &std::path::Path,
) -> Result<(Command, crate::argv_file::ArgvFile)> {
    let file = crate::argv_file::ArgvFile::create(directory, command.get_args())
        .context("writing the Kache engine's arguments to a file")?;
    let mut short = Command::new(command.get_program());
    short.arg(crate::FORWARDED_ARGV_FILE_FLAG).arg(file.path());
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => short.env(key, value),
            None => short.env_remove(key),
        };
    }
    Ok((short, file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn a_command_line_windows_refuses_goes_to_the_engine_in_a_file() {
        use std::ffi::OsStr;
        let directory = tempfile::tempdir().unwrap();
        let long = "x".repeat(40_000);
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg(crate::FORWARDED_ARGV_FLAG)
            .arg(&long)
            .env("KACHE_ARGV_FILE_TEST_SET", "1")
            .env_remove("KACHE_ARGV_FILE_TEST_REMOVED");
        let error = command.status().unwrap_err();
        assert!(crate::argv_file::command_line_too_long(&error), "{error}");

        let (short, file) = through_argv_file(&command, directory.path()).unwrap();
        assert_eq!(short.get_program(), command.get_program());
        assert_eq!(
            short.get_args().collect::<Vec<_>>(),
            [
                OsStr::new(crate::FORWARDED_ARGV_FILE_FLAG),
                file.path().as_os_str()
            ]
        );
        let envs = short.get_envs().collect::<Vec<_>>();
        assert!(envs.contains(&(
            OsStr::new("KACHE_ARGV_FILE_TEST_SET"),
            Some(OsStr::new("1"))
        )));
        assert!(envs.contains(&(OsStr::new("KACHE_ARGV_FILE_TEST_REMOVED"), None)));
        assert_eq!(
            crate::argv_file::read(file.path()).unwrap(),
            [
                OsString::from(crate::FORWARDED_ARGV_FLAG),
                OsString::from(long)
            ]
        );
        let path = file.path().to_owned();
        drop(file);
        assert!(!path.exists());
    }
}

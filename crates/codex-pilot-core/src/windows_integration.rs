use std::ffi::OsStr;
use std::io;

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(not(windows))]
pub const CREATE_NO_WINDOW: u32 = 0;

pub fn apply_no_window(_command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _command.creation_flags(CREATE_NO_WINDOW);
    }
}

pub fn apply_no_window_tokio(_command: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _command.creation_flags(CREATE_NO_WINDOW);
    }
}

pub fn std_command(program: impl AsRef<OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    apply_no_window(&mut command);
    command
}

pub fn tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    apply_no_window_tokio(&mut command);
    command
}

pub fn status_hidden(command: &mut std::process::Command) -> io::Result<std::process::ExitStatus> {
    apply_no_window(command);
    command.status()
}

pub fn output_hidden(command: &mut std::process::Command) -> io::Result<std::process::Output> {
    apply_no_window(command);
    command.output()
}

pub fn spawn_hidden(command: &mut tokio::process::Command) -> io::Result<tokio::process::Child> {
    apply_no_window_tokio(command);
    command.spawn()
}

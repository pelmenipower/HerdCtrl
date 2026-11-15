//! Variables HerdCtrl sets for its own renderer (Vulkan driver filter, OpenGL fallback) must
//! not reach the programs it starts: a game launched by a hotkey would otherwise see only the
//! drivers the editor chose for itself.
use std::{process::Command, sync::OnceLock};

static OWN: OnceLock<Vec<&'static str>> = OnceLock::new();

/// Records the variables this process set for itself. Only the first call counts.
pub fn record(names: Vec<&'static str>) {
    let _ = OWN.set(names);
}

/// Removes the recorded variables from a child's environment.
pub fn clean(command: &mut Command) -> &mut Command {
    remove(command, OWN.get().map(Vec::as_slice).unwrap_or_default())
}

fn remove<'a>(command: &'a mut Command, names: &[&str]) -> &'a mut Command {
    for name in names {
        command.env_remove(name);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn removes_only_the_given_variables() {
        let mut command = Command::new("true");
        command.env("KEEP", "1");
        remove(&mut command, &["VK_LOADER_DRIVERS_SELECT"]);
        let envs: Vec<_> = command.get_envs().collect();
        assert!(envs.contains(&(OsStr::new("KEEP"), Some(OsStr::new("1")))));
        assert!(envs.contains(&(OsStr::new("VK_LOADER_DRIVERS_SELECT"), None)));
    }
}

//! Built-in session commands, matching zlaunch's action items.
#[derive(Clone, Debug, PartialEq)]
pub enum SystemAction {
    Shutdown,
    Reboot,
    Suspend,
    Lock,
    Logout,
    /// Custom command execution (zlaunch's ActionKind::Command). Nothing
    /// built-in or configured produces one, matching the reference, so it is
    /// an internal action type, not a user-facing flow.
    #[allow(dead_code)]
    Command(String),
}

impl SystemAction {
    pub const ALL: [Self; 5] = [
        Self::Shutdown,
        Self::Reboot,
        Self::Suspend,
        Self::Lock,
        Self::Logout,
    ];

    pub fn details(&self) -> (&'static str, &str, &'static str) {
        match self {
            Self::Shutdown => ("Shutdown", "Power off the system", "phosphor:power"),
            Self::Reboot => ("Reboot", "Restart the system", "phosphor:reboot"),
            Self::Suspend => ("Suspend", "Suspend to RAM", "phosphor:moon"),
            Self::Lock => ("Lock Screen", "Lock the session", "phosphor:lock"),
            Self::Logout => ("Log Out", "End the session", "phosphor:sign-out"),
            // zlaunch: name "Custom Command", description the command, terminal icon.
            Self::Command(cmd) => ("Custom Command", cmd, "phosphor:terminal"),
        }
    }

    pub fn command(&self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Shutdown => ("systemctl", &["poweroff"]),
            Self::Reboot => ("systemctl", &["reboot"]),
            Self::Suspend => ("systemctl", &["suspend"]),
            Self::Lock => ("loginctl", &["lock-session"]),
            Self::Logout => ("loginctl", &["terminate-session", "self"]),
            Self::Command(_) => unreachable!("custom commands run through a shell"),
        }
    }

    pub fn execute(self) -> std::io::Result<std::process::Child> {
        if let Self::Command(cmd) = &self {
            // Custom commands are disowned from the daemon, like zlaunch's
            // DetachedProcess::shell.
            use std::os::unix::process::CommandExt;
            let mut command = std::process::Command::new("sh");
            command
                .arg("-c")
                .arg(cmd)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            unsafe {
                command.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
            return command.spawn();
        }
        let (program, args) = self.command();
        std::process::Command::new(program).args(args).spawn()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_uses_only_mock_programs_and_reports_spawn_failure() {
        use std::os::unix::fs::PermissionsExt;
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("calls");
        for program in ["systemctl", "loginctl"] {
            let path = dir.path().join(program);
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"{program} $*\" >> '{}'\n",
                    log.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let old = std::env::var_os("PATH");
        std::env::set_var("PATH", dir.path());
        for action in SystemAction::ALL {
            action.execute().unwrap().wait().unwrap();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if std::fs::read_to_string(&log)
                .unwrap_or_default()
                .lines()
                .count()
                == 5
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "mock children did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let mut lines: Vec<_> = std::fs::read_to_string(log)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        lines.sort();
        assert_eq!(
            lines,
            [
                "loginctl lock-session",
                "loginctl terminate-session self",
                "systemctl poweroff",
                "systemctl reboot",
                "systemctl suspend"
            ]
        );
        std::fs::remove_file(dir.path().join("systemctl")).unwrap();
        assert!(SystemAction::Shutdown.execute().is_err());
        if let Some(old) = old {
            std::env::set_var("PATH", old)
        } else {
            std::env::remove_var("PATH")
        };
    }

    #[test]
    fn builtin_commands_match_the_reference_without_running_them() {
        assert_eq!(
            SystemAction::ALL.map(|a| a.command()),
            [
                ("systemctl", &["poweroff"][..]),
                ("systemctl", &["reboot"][..]),
                ("systemctl", &["suspend"][..]),
                ("loginctl", &["lock-session"][..]),
                ("loginctl", &["terminate-session", "self"][..]),
            ]
        );
    }
}

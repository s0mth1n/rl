//! Detached desktop application launching, following zlaunch's process helpers.
use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;

static SESSION: OnceLock<HashMap<String, String>> = OnceLock::new();

fn merge_environment(
    current: impl IntoIterator<Item = (String, String)>,
    systemd: &str,
) -> HashMap<String, String> {
    let mut env: HashMap<_, _> = current.into_iter().collect();
    for line in systemd.lines() {
        if let Some((key, value)) = line.split_once('=') {
            env.entry(key.into())
                .or_insert_with(|| value.trim_matches('"').trim_matches('\'').into());
        }
    }
    env
}

pub fn capture_environment() {
    SESSION.get_or_init(|| {
        let systemd = Command::new("timeout")
            .args(["1", "systemctl", "--user", "show-environment"])
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        merge_environment(std::env::vars(), &systemd)
    });
}

fn command(exec: &str, terminal: bool, env: &HashMap<String, String>) -> Result<Command, String> {
    let cleaned = crate::items::exec_line(exec);
    let mut command = if terminal {
        let terminal = match env.get("TERMINAL") {
            Some(value) => value.clone(),
            None => {
                if !Command::new("which")
                    .arg("xterm")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success())
                {
                    return Err("No terminal emulator found".into());
                }
                "xterm".into()
            }
        };
        let mut command = Command::new(terminal);
        command.arg("-e").arg(cleaned);
        command
    } else {
        let mut parts = cleaned.split_whitespace();
        let program = parts.next().ok_or("Empty application command")?;
        let mut command = Command::new(program);
        command.args(parts);
        command
    };
    command.env_clear().envs(env);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    Ok(command)
}

pub fn application(exec: &str, terminal: bool) -> Result<Child, String> {
    let env = SESSION.get_or_init(|| std::env::vars().collect());
    command(exec, terminal, env)?
        .spawn()
        .map_err(|e| e.to_string())
}

pub fn reap(children: &mut Vec<Child>) {
    children.retain_mut(|child| match child.try_wait() {
        Ok(Some(_)) => false,
        Ok(None) => true,
        Err(error) => {
            eprintln!("zl: cannot reap application: {error}");
            false
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finished_applications_are_reaped_without_waiting_for_running_ones() {
        let env: HashMap<_, _> = std::env::vars().collect();
        let mut children = vec![
            command("/bin/sleep 2", false, &env)
                .unwrap()
                .spawn()
                .unwrap(),
            command("/bin/true", false, &env).unwrap().spawn().unwrap(),
        ];
        let finished_pid = children[1].id();
        let start = std::time::Instant::now();
        while children.len() != 1 && start.elapsed() < std::time::Duration::from_secs(1) {
            reap(&mut children);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(children.len(), 1);
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        assert!(!std::path::Path::new(&format!("/proc/{finished_pid}")).exists());
        children[0].kill().unwrap();
        children[0].wait().unwrap();
        reap(&mut children);
        assert!(children.is_empty());
    }

    #[test]
    fn argv_matches_reference_without_shell_expansion() {
        let env = HashMap::new();
        let command = command("/bin/echo 'two words' $HOME pre%fpost %%", false, &env).unwrap();
        assert_eq!(command.get_program(), "/bin/echo");
        assert_eq!(
            command
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["'two", "words'", "$HOME", "prepost", "%%"]
        );
        assert!(command.get_envs().all(|(_, value)| value.is_none()));
        assert!(super::command("  %f ", false, &env).is_err());
    }
    #[test]
    fn terminal_passes_one_exec_argument_and_current_environment_wins() {
        let env = merge_environment(
            [
                ("TERMINAL".into(), "foot".into()),
                ("WAYLAND_DISPLAY".into(), "live".into()),
            ],
            "TERMINAL=xterm\nWAYLAND_DISPLAY=stale\nQT_QPA_PLATFORMTHEME='qt6ct'\nBROKEN\n",
        );
        let command = command("editor %U --new", true, &env).unwrap();
        assert_eq!(command.get_program(), "foot");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-e", "editor  --new"]
        );
        assert_eq!(env["WAYLAND_DISPLAY"], "live");
        assert_eq!(env["QT_QPA_PLATFORMTHEME"], "qt6ct");
    }
    #[test]
    fn launched_child_has_its_own_session_and_failures_return_errors() {
        let env: HashMap<_, _> = std::env::vars().collect();
        let child = command("/bin/sleep 2", false, &env)
            .unwrap()
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        let mut child = child;
        std::thread::sleep(std::time::Duration::from_millis(25));
        assert_eq!(unsafe { libc::getsid(pid) }, pid);
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(command("/not/a/program", false, &env)
            .unwrap()
            .spawn()
            .is_err());
    }
}

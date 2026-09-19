mod dbus;
mod limits;
mod x11;

use dbus::*;
use limits::*;
use niri_ipc::state::{EventStreamState, EventStreamStatePart};
use niri_ipc::{Request, Window, socket::Socket};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use x11::*;
use zbus::blocking::Connection;

/// Reads excluded app IDs from `NIRI_FOCUSED_BOOSTER_EXCLUDE` (comma-separated) and/or CLI args
/// (also comma-separated, can be repeated). Matching is a case-insensitive substring match
/// against the window's `app_id`, so e.g. "steam" also matches "steam_app_12345".
fn load_exclusions() -> Vec<String> {
    let mut exclusions: Vec<String> = std::env::args()
        .skip(1)
        .flat_map(|arg| arg.split(',').map(str::to_owned).collect::<Vec<_>>())
        .collect();

    if let Ok(env_value) = std::env::var("NIRI_FOCUSED_BOOSTER_EXCLUDE") {
        exclusions.extend(env_value.split(',').map(str::to_owned));
    }

    exclusions.into_iter().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect()
}

fn is_excluded(app_id: Option<&str>, exclusions: &[String]) -> bool {
    let Some(app_id) = app_id else {
        return false;
    };

    let app_id = app_id.to_lowercase();
    exclusions.iter().any(|excluded| app_id.contains(excluded.as_str()))
}

/// PIDs of every window that should currently be boosted: fullscreen and not excluded.
/// A `HashSet` because with multiple monitors more than one window can be fullscreen at once.
fn compute_boost_pids(state: &EventStreamState, exclusions: &[String]) -> HashSet<i32> {
    state
        .windows
        .windows
        .values()
        .filter(|window: &&Window| window.is_fullscreen)
        .filter(|window| !is_excluded(window.app_id.as_deref(), exclusions))
        .filter_map(|window| window.pid)
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let boosted_limits =
        parse_limits_file(Path::new("/sys/fs/cgroup/dmem.capacity")).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Failed to read /sys/fs/cgroup/dmem.capacity")
        })?;
    let non_boosted_limits: DmemLimit = boosted_limits.keys().map(|key| (key.clone(), 0)).collect();
    let exclusions = load_exclusions();

    let mut event_socket = Socket::connect()?;
    let conn = Connection::session()?;

    let reply = event_socket.send(Request::EventStream)?;
    if let Err(message) = reply {
        return Err(
            io::Error::other(format!("Failed to request niri event stream: {message}")).into()
        );
    }

    // Currently-boosted windows, keyed by their original (pre-xwayland-satellite-resolved) PID.
    let boosted_paths: Arc<Mutex<HashMap<i32, PathBuf>>> = Arc::new(Mutex::new(HashMap::new()));

    {
        let boosted_paths = Arc::clone(&boosted_paths);
        let cleanup_non_boosted_limits = non_boosted_limits.clone();

        ctrlc::set_handler(move || {
            let paths = match boosted_paths.lock() {
                Ok(paths) => paths.clone(),
                Err(error) => {
                    eprintln!("WARNING: Failed to lock boosted paths during cleanup: {error}");
                    std::process::exit(1);
                }
            };

            for path in paths.values() {
                if let Err(error) = set_dmem_low(path, &cleanup_non_boosted_limits) {
                    eprintln!("WARNING: Failed to cleanup dmem.low at {}: {error}", path.display());
                }
            }

            std::process::exit(0);
        })?;
    }

    let mut state = EventStreamState::default();
    // Caches PID -> resolved dmem.low path (or None if resolution failed), so we don't hit
    // xwayland-satellite/D-Bus again for a window we've already resolved.
    let mut pid_path_cache: HashMap<i32, Option<PathBuf>> = HashMap::new();

    let mut read_event = event_socket.read_events();
    while let Ok(event) = read_event() {
        state.apply(event);

        let target_pids = compute_boost_pids(&state, &exclusions);

        // Drop stale cache entries for windows that are no longer targets (closed, unfullscreened,
        // or newly excluded).
        pid_path_cache.retain(|pid, _| target_pids.contains(pid));

        let mut target_paths: HashMap<i32, PathBuf> = HashMap::new();
        for pid in &target_pids {
            let resolved = pid_path_cache.entry(*pid).or_insert_with(|| {
                find_real_pid(*pid).and_then(|real_pid| {
                    match dmem_low_path_for_pid(&conn, real_pid) {
                        Ok(path) => Some(path),
                        Err(error) => {
                            eprintln!("WARNING: Failed to resolve cgroup for PID {real_pid}: {error}");
                            None
                        }
                    }
                })
            });

            if let Some(path) = resolved {
                target_paths.insert(*pid, path.clone());
            }
        }

        let mut boosted = match boosted_paths.lock() {
            Ok(boosted) => boosted,
            Err(error) => {
                eprintln!("WARNING: Failed to lock boosted paths: {error}");
                continue;
            }
        };

        // Un-boost anything that's no longer a target.
        boosted.retain(|pid, path| {
            if target_paths.contains_key(pid) {
                return true;
            }

            if let Err(error) = set_dmem_low(path, &non_boosted_limits) {
                eprintln!(
                    "WARNING: Failed to set non-boosted dmem.low at {}: {error}",
                    path.display()
                );
            }

            false
        });

        // Boost any new targets.
        for (pid, path) in &target_paths {
            if boosted.contains_key(pid) {
                continue;
            }

            if let Err(error) = set_dmem_low(path, &boosted_limits) {
                eprintln!("WARNING: Failed to set boosted dmem.low at {}: {error}", path.display());
                continue;
            }

            boosted.insert(*pid, path.clone());
        }
    }

    Ok(())
}

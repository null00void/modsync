// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Real fix for a real bug hit on a real AMD/Mesa desktop (RX 7700 XT):
/// installing the AppImage gave a blank white window, WebKitGTK aborting
/// with "Could not create default EGL display: EGL_BAD_PARAMETER".
///
/// Root cause, confirmed by extracting the actual shipped AppImage and
/// testing it directly: the AppImage bundles its own private
/// `libwayland-client.so`/`libwayland-egl.so` (pulled in as GTK/WebKit
/// dependencies), and the bundle's `LD_LIBRARY_PATH` prioritizes them
/// over the system's copies -- even though the AppImage's own generated
/// launcher already forces GDK onto the X11 backend for an unrelated,
/// separate known issue. WebKitGTK's EGL platform probing fails against
/// the bundled Wayland libs regardless. Confirmed fixed by either
/// deleting the bundled libs entirely or `LD_PRELOAD`-ing the system's
/// absolute path -- but there's no supported Tauri/linuxdeploy config to
/// exclude specific bundled libraries at build time, and the system
/// library path differs per distro (Fedora: `/usr/lib64`, Debian/Ubuntu:
/// `/usr/lib/x86_64-linux-gnu`, etc.), so it can't be hardcoded either.
///
/// So it's done here at runtime instead: detect we're running from an
/// AppImage (every AppImage sets `APPDIR`), find the system's own
/// `libwayland-client.so` via `ldconfig -p` (present on every Linux
/// distro), and re-exec this same binary with `LD_PRELOAD` set to it.
/// `LD_PRELOAD` only takes effect for a new process image -- by the time
/// `main()` runs the dynamic linker has already resolved this process's
/// libraries -- hence the re-exec rather than just setting the env var
/// in place. A sentinel env var stops it from looping.
#[cfg(target_os = "linux")]
fn fix_appimage_wayland_egl() {
    if std::env::var_os("APPDIR").is_none() || std::env::var_os("MODSYNC_REEXECED").is_some() {
        return;
    }

    let ldconfig_output = ["ldconfig", "/sbin/ldconfig", "/usr/sbin/ldconfig"]
        .iter()
        .find_map(|cmd| std::process::Command::new(cmd).arg("-p").output().ok());
    let Some(output) = ldconfig_output else {
        eprintln!("modsync: could not run ldconfig, skipping Wayland library fix");
        return;
    };

    let listing = String::from_utf8_lossy(&output.stdout);
    let system_libwayland = listing.lines().find_map(|line| {
        let (name, path) = line.trim().split_once(" => ")?;
        name.trim()
            .starts_with("libwayland-client.so")
            .then(|| path.trim().to_string())
    });
    let Some(system_libwayland) = system_libwayland else {
        eprintln!("modsync: system libwayland-client.so not found, skipping Wayland library fix");
        return;
    };

    let existing_preload = std::env::var("LD_PRELOAD").unwrap_or_default();
    let new_preload = if existing_preload.is_empty() {
        system_libwayland
    } else {
        format!("{existing_preload}:{system_libwayland}")
    };

    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(
        std::env::current_exe().unwrap_or_else(|_| "modsync".into()),
    )
    .args(std::env::args_os().skip(1))
    .env("LD_PRELOAD", new_preload)
    .env("MODSYNC_REEXECED", "1")
    .exec();
    // exec() only returns on failure. Fall through and run with the
    // original (possibly broken) libraries rather than not starting.
    eprintln!("modsync: failed to re-exec with Wayland library fix: {err}");
}

fn main() {
    #[cfg(target_os = "linux")]
    {
        fix_appimage_wayland_egl();

        // Extra safety nets for other GPU/driver combos -- harmless to
        // set even where they don't turn out to be the deciding factor.
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }

    modsync_lib::run()
}

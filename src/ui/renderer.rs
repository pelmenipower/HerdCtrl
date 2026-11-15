//! Renderer choice for the editor window.
//!
//! eframe's OpenGL renderer loads Mesa's `libgallium`, which links `libLLVM` (about 40 MB
//! resident). wgpu on Vulkan avoids that, as long as the Vulkan loader does not open an
//! LLVM-linked driver while enumerating devices: lavapipe (software) and RADV. The editor
//! therefore lets the loader open only the driver of the primary GPU's vendor, and falls back
//! to OpenGL in a fresh process when no Vulkan device works.
use crate::os::child_env;
use eframe::{egui_wgpu, wgpu, Renderer};
use std::{os::unix::process::CommandExt, path::Path};

/// Set by the fallback, or by hand, to force the OpenGL renderer.
const RENDERER_ENV: &str = "HERDCTRL_RENDERER";
/// Marks a process started by the fallback, so it knows `HERDCTRL_RENDERER` is its own.
const FALLBACK_ENV: &str = "HERDCTRL_RENDERER_FALLBACK";
const DRIVERS_SELECT_ENV: &str = "VK_LOADER_DRIVERS_SELECT";
const DRIVERS_DISABLE_ENV: &str = "VK_LOADER_DRIVERS_DISABLE";
/// Loader variables through which the user already chose drivers.
const USER_DRIVER_ENVS: [&str; 5] = [
    DRIVERS_SELECT_ENV,
    DRIVERS_DISABLE_ENV,
    "VK_ICD_FILENAMES",
    "VK_DRIVER_FILES",
    "VK_ADD_DRIVER_FILES",
];

const INTEL: u16 = 0x8086;
/// Vulkan driver manifests (`/usr/share/vulkan/icd.d`) per PCI vendor ID.
const VENDOR_DRIVERS: [(u16, &[&str]); 4] = [
    (INTEL, &["*intel*"]),
    (0x1002, &["*radeon*", "*amd*"]),
    (0x10de, &["*nouveau*", "*nvidia*"]),
    (0x1af4, &["*virtio*"]),
];

/// A DRM device: PCI vendor ID and whether the firmware booted on it (`boot_vga`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gpu {
    pub vendor: u16,
    pub primary: bool,
}

/// Vendors whose drivers the editor may open. Only the primary GPU's, so a sleeping discrete
/// GPU is not woken and RADV (which links LLVM) stays closed on hybrid laptops. Without a
/// primary GPU, an Intel iGPU is preferred, otherwise every vendor counts.
fn wanted_vendors(gpus: &[Gpu]) -> Vec<u16> {
    let pick = |primary_only: bool| -> Vec<u16> {
        let mut vendors: Vec<u16> = gpus
            .iter()
            .filter(|gpu| gpu.primary || !primary_only)
            .map(|gpu| gpu.vendor)
            .collect();
        vendors.sort_unstable();
        vendors.dedup();
        vendors
    };
    let primary = pick(true);
    if !primary.is_empty() {
        return primary;
    }
    if gpus.iter().any(|gpu| gpu.vendor == INTEL) {
        return vec![INTEL];
    }
    pick(false)
}

/// The Vulkan loader variable and value that keep LLVM-linked and unneeded drivers closed.
///
/// With a known GPU vendor only its drivers are selected, which also leaves out lavapipe and
/// every other vendor's driver (the loader maps each one it opens, about 2 MB each for Mesa).
/// Without one, only the LLVM-linked lavapipe and RADV are disabled, so Vulkan stays usable.
/// RADV itself links LLVM, so AMD systems keep that cost.
pub fn loader_filter(gpus: &[Gpu]) -> (&'static str, String) {
    let vendors = wanted_vendors(gpus);
    let selected: Vec<&str> = VENDOR_DRIVERS
        .iter()
        .filter(|(vendor, _)| vendors.contains(vendor))
        .flat_map(|(_, globs)| globs.iter().copied())
        .collect();
    if selected.is_empty() {
        (DRIVERS_DISABLE_ENV, "*lvp*,*radeon*".into())
    } else {
        (DRIVERS_SELECT_ENV, selected.join(","))
    }
}

/// The DRM devices with a PCI vendor; connectors and platform devices have none.
fn gpus(drm: &Path) -> Vec<Gpu> {
    let Ok(entries) = std::fs::read_dir(drm) else {
        return Vec::new();
    };
    let read = |path: &Path| std::fs::read_to_string(path).ok();
    let mut gpus: Vec<Gpu> = entries
        .flatten()
        .filter_map(|entry| {
            let device = entry.path().join("device");
            let raw = read(&device.join("vendor"))?;
            let vendor = u16::from_str_radix(raw.trim().trim_start_matches("0x"), 16).ok()?;
            let primary = read(&device.join("boot_vga")).is_some_and(|v| v.trim() == "1");
            Some(Gpu { vendor, primary })
        })
        .collect();
    gpus.sort_by_key(|gpu| (gpu.vendor, gpu.primary));
    gpus
}

fn use_glow() -> bool {
    std::env::var_os(RENDERER_ENV).is_some_and(|value| value == "glow")
}

/// Restricts the Vulkan drivers the loader opens. Call only for the editor, at the very start
/// of `main` before any thread exists. A driver choice the user made is left alone. Whatever
/// this sets is recorded, so programs HerdCtrl starts do not inherit it.
pub fn prepare() {
    if std::env::var_os(FALLBACK_ENV).is_some() {
        child_env::record(vec![RENDERER_ENV, FALLBACK_ENV]);
        return;
    }
    let user_choice = USER_DRIVER_ENVS
        .iter()
        .any(|name| std::env::var_os(name).is_some());
    if use_glow() || user_choice {
        return;
    }
    let (name, value) = loader_filter(&gpus(Path::new("/sys/class/drm")));
    std::env::set_var(name, value);
    child_env::record(vec![name]);
}

/// Sets the renderer in `options`: wgpu on Vulkan, or OpenGL after a fallback.
pub fn configure(options: &mut eframe::NativeOptions) {
    if use_glow() {
        options.renderer = Renderer::Glow;
        return;
    }
    options.renderer = Renderer::Wgpu;
    options.wgpu_options.wgpu_setup =
        egui_wgpu::WgpuSetup::CreateNew(egui_wgpu::WgpuSetupCreateNew {
            instance_descriptor: wgpu::InstanceDescriptor {
                backends: wgpu::Backends::VULKAN,
                ..Default::default()
            },
            // The editor is small; the integrated GPU is enough and keeps a discrete one asleep.
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        });
}

/// Handles an error from `run_native`. If wgpu failed to start (no usable Vulkan device),
/// replaces this process with one that uses OpenGL; eframe reports that before the window is
/// shown. Any other error, or a failure of OpenGL itself, is returned unchanged.
pub fn fall_back_to_glow(error: eframe::Error) -> Box<dyn std::error::Error> {
    if !matches!(error, eframe::Error::Wgpu(_)) || use_glow() {
        return error.to_string().into();
    }
    eprintln!("Vulkan renderer unavailable ({error}); using OpenGL");
    let mut args = std::env::args_os();
    // /proc/self/exe still works when the binary was replaced while running.
    let mut command = std::process::Command::new("/proc/self/exe");
    if let Some(arg0) = args.next() {
        command.arg0(arg0);
    }
    // Locks are opened with O_CLOEXEC, so the new process can take them again.
    let failed = command
        .args(args)
        .env(RENDERER_ENV, "glow")
        .env(FALLBACK_ENV, "1")
        .exec();
    format!("{error}; starting with OpenGL failed: {failed}").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(vendor: u16, primary: bool) -> Gpu {
        Gpu { vendor, primary }
    }

    #[test]
    fn selects_the_primary_gpu_vendor() {
        assert_eq!(
            loader_filter(&[gpu(0x8086, true)]),
            (DRIVERS_SELECT_ENV, "*intel*".into())
        );
        assert_eq!(
            loader_filter(&[gpu(0x1002, true)]),
            (DRIVERS_SELECT_ENV, "*radeon*,*amd*".into())
        );
        // Desktop with a discrete NVIDIA card driving the screens and the iGPU enabled.
        assert_eq!(
            loader_filter(&[gpu(0x8086, false), gpu(0x10de, true)]),
            (DRIVERS_SELECT_ENV, "*nouveau*,*nvidia*".into())
        );
    }

    #[test]
    fn keeps_the_discrete_gpu_closed_on_hybrid_laptops() {
        for discrete in [0x10de, 0x1002] {
            let gpus = [gpu(0x8086, true), gpu(discrete, false)];
            assert_eq!(loader_filter(&gpus), (DRIVERS_SELECT_ENV, "*intel*".into()));
        }
        // No boot_vga at all: the Intel iGPU still wins.
        let gpus = [gpu(0x8086, false), gpu(0x10de, false)];
        assert_eq!(loader_filter(&gpus), (DRIVERS_SELECT_ENV, "*intel*".into()));
    }

    #[test]
    fn disables_only_llvm_drivers_without_a_known_gpu() {
        let fallback = (DRIVERS_DISABLE_ENV, "*lvp*,*radeon*".to_string());
        assert_eq!(loader_filter(&[]), fallback);
        assert_eq!(loader_filter(&[gpu(0x1234, true)]), fallback);
    }

    #[test]
    fn reads_vendors_and_boot_vga_from_sysfs() {
        let root = std::env::temp_dir().join(format!("herdctrl-drm-{}", std::process::id()));
        for (card, vendor, boot) in [
            ("card0", "0x8086\n", Some("1\n")),
            ("card1", "0x10de\n", Some("0\n")),
            ("renderD128", "0x8086\n", None),
        ] {
            let device = root.join(card).join("device");
            std::fs::create_dir_all(&device).unwrap();
            std::fs::write(device.join("vendor"), vendor).unwrap();
            if let Some(boot) = boot {
                std::fs::write(device.join("boot_vga"), boot).unwrap();
            }
        }
        // A connector entry has no device vendor.
        std::fs::create_dir_all(root.join("card0-eDP-1")).unwrap();
        assert_eq!(
            gpus(&root),
            [gpu(0x10de, false), gpu(0x8086, false), gpu(0x8086, true)]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

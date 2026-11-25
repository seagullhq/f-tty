//! Process-wide memory tuning at startup and when a tab closes.
//!
//! The Vulkan loader maps every installed ICD, including Mesa's software
//! rasterizer, which drags all of LLVM into RAM (~30 MB RSS on this machine).
//! Only the driver of the GPU driving the display is ever used, so tell the
//! loader to skip the rest. An existing `VK_LOADER_DRIVERS_SELECT` wins, and
//! when no driver can be identified we leave the loader alone.

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub fn restrict_vulkan_drivers() {
    if std::env::var_os("VK_LOADER_DRIVERS_SELECT").is_some() {
        return;
    }
    let pattern = display_pattern().or_else(render_node_patterns);
    if let Some(pattern) = pattern {
        // SAFETY: called from main before any other thread starts.
        unsafe { std::env::set_var("VK_LOADER_DRIVERS_SELECT", pattern) };
    }
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
pub fn restrict_vulkan_drivers() {}

/// glibc hands out one malloc arena per thread (up to 8 per core) and keeps
/// their freed pages mapped. The app has many short-lived threads, so cap the
/// arenas before any thread starts and let them share.
pub fn limit_allocator_arenas() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        if std::env::var_os("MALLOC_ARENA_MAX").is_none() {
            unsafe extern "C" {
                fn mallopt(parameter: std::ffi::c_int, value: std::ffi::c_int) -> std::ffi::c_int;
            }
            // SAFETY: first thing in main, before any other thread exists.
            // -8 is M_ARENA_MAX from glibc's malloc.h.
            unsafe { mallopt(-8, 2) };
        }
    }
}

/// Returns the pages freed by the last tab (scrollback, PTY buffers) to the
/// OS; both allocators otherwise keep them for reuse.
pub fn release_free_pages() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        // SAFETY: glibc's allocator is thread safe.
        unsafe { malloc_trim(0) };
    }

    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        // SAFETY: a null zone means "all zones", and the call only releases
        // free pages.
        unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
    }
}

/// ICD pattern for the card with a connector attached to a display.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn display_pattern() -> Option<String> {
    let connected = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let card = name.split_once('-')?.0;
            let status = std::fs::read_to_string(entry.path().join("status")).ok()?;
            (status.trim() == "connected").then(|| card.to_string())
        })
        .next()?;
    let driver =
        std::fs::read_to_string(format!("/sys/class/drm/{connected}/device/uevent")).ok()?;
    let driver = driver
        .lines()
        .find_map(|line| line.strip_prefix("DRIVER="))?;
    driver_pattern(driver).map(str::to_string)
}

/// Fallback: patterns for every render node, but only if they are all known.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn render_node_patterns() -> Option<String> {
    let mut patterns: Vec<&str> = Vec::new();
    for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("renderD") {
            continue;
        }
        let uevent = std::fs::read_to_string(entry.path().join("device/uevent")).ok()?;
        let driver = uevent
            .lines()
            .find_map(|line| line.strip_prefix("DRIVER="))?;
        patterns.push(driver_pattern(driver)?);
    }
    patterns.sort_unstable();
    patterns.dedup();
    (!patterns.is_empty()).then(|| patterns.join(","))
}

/// Kernel DRM driver name -> ICD library globs.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn driver_pattern(driver: &str) -> Option<&'static str> {
    Some(match driver {
        "i915" | "xe" => "*intel*",
        "amdgpu" | "radeon" => "*radeon*,*amdvlk*",
        "nvidia-drm" => "*nvidia*",
        "nouveau" => "*nouveau*",
        "virtio_gpu" => "*virtio*",
        "v3d" | "vc4" | "vc5" => "*broadcom*",
        "msm" => "*freedreno*",
        "panfrost" | "panthor" => "*panfrost*",
        "asahi" => "*asahi*",
        _ => return None,
    })
}

#[cfg(all(test, any(target_os = "linux", target_os = "freebsd")))]
mod tests {
    use super::driver_pattern;

    #[::core::prelude::v1::test]
    fn known_drivers_map_to_icd_globs() {
        assert_eq!(driver_pattern("i915"), Some("*intel*"));
        assert_eq!(driver_pattern("amdgpu"), Some("*radeon*,*amdvlk*"));
        assert_eq!(driver_pattern("unknown"), None);
    }
}

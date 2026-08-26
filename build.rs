use std::process::Command;

/// Ask pkg-config for the installed OpenCV version.
///
/// OpenCV 5 installs `opencv5.pc` and drops `opencv4.pc` entirely, so both
/// names have to be probed. Newest first, so a box with both files reports the
/// version the `opencv` crate itself will link against.
fn detect_opencv_version() -> Option<String> {
    ["opencv5", "opencv4"].into_iter().find_map(|pc| {
        let output = Command::new("pkg-config")
            .args(["--modversion", pc])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    })
}

fn main() {
    // Declare custom cfg values for check-cfg
    println!("cargo::rustc-check-cfg=cfg(cvt_color4)");
    println!("cargo::rustc-check-cfg=cfg(cvt_color5)");

    let version = detect_opencv_version().unwrap_or_else(|| {
        println!(
            "cargo:warning=Could not detect OpenCV version via pkg-config, defaulting to 4.12"
        );
        "4.12.0".to_owned()
    });

    println!("cargo:warning=Detected OpenCV version: {version}");

    // OpenCV 4.10 and earlier take 4 arguments to cvt_color; 4.11+ and 5.x take
    // a fifth AlgorithmHint argument.
    let mut parts = version.split('.');
    let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(4);
    let minor: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(12);

    if major == 4 && minor <= 10 {
        println!("cargo:rustc-cfg=cvt_color4");
        println!("cargo:warning=Using 4-parameter cvt_color (OpenCV 4.10 API)");
    } else {
        println!("cargo:rustc-cfg=cvt_color5");
        println!("cargo:warning=Using 5-parameter cvt_color (OpenCV 4.11+ API)");
    }

    // Re-run build script if the pkg-config search path changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    // Capture rustc version for metrics
    let rustc_output = Command::new("rustc")
        .arg("--version")
        .output()
        .expect("Failed to run rustc --version");
    let rustc_version = String::from_utf8_lossy(&rustc_output.stdout)
        .trim()
        .to_string();
    println!("cargo:rustc-env=RUSTC_VERSION={rustc_version}");
}

use std::process::Command;

fn main() {
    // Declare custom cfg values for check-cfg
    println!("cargo::rustc-check-cfg=cfg(cvt_color4)");
    println!("cargo::rustc-check-cfg=cfg(cvt_color5)");

    // Get OpenCV version from pkg-config
    let output = Command::new("pkg-config")
        .args(["--modversion", "opencv4"])
        .output();

    let version = match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            // If pkg-config fails, default to 4.12
            eprintln!("cargo:warning=Could not detect OpenCV version via pkg-config, defaulting to 4.12");
            "4.12.0".to_string()
        }
    };

    println!("cargo:warning=Detected OpenCV version: {}", version);

    // Parse major.minor version
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        let major: u32 = parts[0].parse().unwrap_or(4);
        let minor: u32 = parts[1].parse().unwrap_or(12);

        // Set cfg based on version
        // OpenCV 4.10 and earlier use 4-parameter cvt_color
        // OpenCV 4.12+ uses 5-parameter cvt_color
        if major == 4 && minor <= 10 {
            println!("cargo:rustc-cfg=cvt_color4");
            println!("cargo:warning=Using 4-parameter cvt_color (OpenCV 4.10 API)");
        } else {
            println!("cargo:rustc-cfg=cvt_color5");
            println!("cargo:warning=Using 5-parameter cvt_color (OpenCV 4.12+ API)");
        }
    } else {
        // Default to 5-parameter version if parsing fails
        println!("cargo:rustc-cfg=cvt_color5");
        println!("cargo:warning=Could not parse OpenCV version, defaulting to 5-parameter cvt_color");
    }

    // Re-run build script if opencv4.pc changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
}

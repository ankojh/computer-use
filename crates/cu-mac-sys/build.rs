//! Builds the CuMac Swift package and links it (plus the Swift runtime and system frameworks).

use std::env;
use std::path::PathBuf;
use std::process::Command;

const FRAMEWORKS: &[&str] = &[
    "ScreenCaptureKit",
    "CoreGraphics",
    "CoreMedia",
    "CoreVideo",
    "ApplicationServices",
    "AppKit",
    "ImageIO",
    "UniformTypeIdentifiers",
    "Accelerate",
    "Metal",
    "MetalPerformanceShaders",
    "AVFoundation",
    "VideoToolbox",
    "Carbon",
    "Foundation",
];

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        // The core crates stay portable; nothing to build off macOS.
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let package = manifest_dir
        .join("../../swift/CuMac")
        .canonicalize()
        .expect("swift/CuMac not found");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let scratch = out_dir.join("swift-build");
    let config = match env::var("PROFILE").as_deref() {
        Ok("release") => "release",
        _ => "debug",
    };

    let swift_args = |extra: &[&str]| {
        let mut cmd = Command::new("swift");
        cmd.arg("build")
            .args(["-c", config])
            .arg("--package-path")
            .arg(&package)
            .arg("--scratch-path")
            .arg(&scratch)
            .args(extra)
            // Cargo's env can leak into swift's own toolchain lookups; keep it clean.
            .env_remove("SDKROOT");
        cmd
    };

    let status = swift_args(&[])
        .status()
        .expect("failed to run `swift build`");
    assert!(status.success(), "`swift build` failed");

    let bin_path = run(swift_args(&["--show-bin-path"]));
    println!("cargo:rustc-link-search=native={}", bin_path.trim());
    println!("cargo:rustc-link-lib=static=CuMac");

    for fw in FRAMEWORKS {
        println!("cargo:rustc-link-lib=framework={fw}");
    }

    // Swift runtime: the static lib carries autolink directives (-lswiftCore, ...); give the
    // linker the directories that contain those libraries/tbds.
    let info = run({
        let mut c = Command::new("swiftc");
        c.arg("-print-target-info");
        c
    });
    for path in runtime_library_paths(&info) {
        println!("cargo:rustc-link-search=native={path}");
    }
    let sdk = run({
        let mut c = Command::new("xcrun");
        c.args(["--sdk", "macosx", "--show-sdk-path"]);
        c
    });
    println!(
        "cargo:rustc-link-search=native={}/usr/lib/swift",
        sdk.trim()
    );
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    println!(
        "cargo:rerun-if-changed={}",
        package.join("Package.swift").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        package.join("Sources").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        package.join("include").display()
    );
}

fn run(mut cmd: Command) -> String {
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to run {cmd:?}: {e}"));
    assert!(
        out.status.success(),
        "{cmd:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("non-UTF-8 output")
}

/// Extracts `paths.runtimeLibraryPaths` from `swiftc -print-target-info` without a JSON dependency.
fn runtime_library_paths(info: &str) -> Vec<String> {
    let Some(start) = info.find("\"runtimeLibraryPaths\"") else {
        return Vec::new();
    };
    let rest = &info[start..];
    let (Some(open), Some(close)) = (rest.find('['), rest.find(']')) else {
        return Vec::new();
    };
    rest[open + 1..close]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

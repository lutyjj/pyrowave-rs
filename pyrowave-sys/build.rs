//! Build the pinned PyroWave C API as static archives through CMake and generate
//! Rust bindings for it. CMake owns the patched copy in its build directory.
//!
//! Everything compiles from initialized submodules, including Vulkan headers, so
//! the build needs no network and no system PyroWave; bindgen needs a libclang.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
	println!("cargo:rerun-if-changed=wrapper.h");
	println!("cargo:rerun-if-changed=quality.c");
	println!("cargo:rerun-if-changed=CMakeLists.txt");
	println!("cargo:rerun-if-changed=THIRD-PARTY-NOTICES");
	println!("cargo:rerun-if-changed=UPSTREAM_REVISIONS");
	println!("cargo:rerun-if-changed=upstream");
	println!("cargo:rerun-if-changed=patches");

	let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
	// Submodule HEADs live outside their source trees. Track them when Git
	// metadata is available, including pin changes that leave file contents equal.
	for source in [
		"pyrowave",
		"Granite",
		"Granite/third_party/volk",
		"Granite/third_party/khronos/vulkan-headers",
	] {
		let checkout = manifest_dir.join("upstream").join(source);
		if !checkout.join(".git").exists() {
			continue;
		}
		let Ok(output) = Command::new("git")
			.arg("-C")
			.arg(&checkout)
			.args(["rev-parse", "--absolute-git-dir"])
			.output()
		else {
			continue;
		};
		if !output.status.success() {
			continue;
		}
		let git_dir = PathBuf::from(String::from_utf8(output.stdout).expect("Git metadata path").trim());
		for entry in ["HEAD", "refs", "packed-refs"] {
			let path = git_dir.join(entry);
			if path.exists() {
				println!("cargo:rerun-if-changed={}", path.display());
			}
		}
	}
	assert_eq!(
		env::var("CARGO_CFG_TARGET_OS").unwrap(),
		"linux",
		"this native build supports Linux"
	);

	// Always Release: the hot path is GPU shaders, a debug host build buys nothing.
	let dst = cmake::Config::new(&manifest_dir)
		.profile("Release")
		.build_target("pyrowave-capi")
		.build();
	let build = dst.join("build");
	let vendor = build.join("vendor/pyrowave");
	let vk_include = vendor.join("Granite/third_party/khronos/vulkan-headers/include");
	let provenance = std::fs::read_to_string(vendor.join("VENDOR.txt")).expect("read vendor provenance");
	let revision = provenance
		.lines()
		.find_map(|line| {
			line.strip_prefix("pyrowave:")
				.and_then(|value| value.split_whitespace().next())
		})
		.expect("find PyroWave revision");
	assert!(
		revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
		"invalid revision"
	);
	println!("cargo:rustc-env=PYROWAVE_UPSTREAM_REVISION={revision}");
	println!("cargo:rustc-env=PYROWAVE_BITSTREAM_ID={}", &revision[..8]);

	// Dependents before dependencies: GNU ld resolves static archives in order.
	println!("cargo:rustc-link-search=native={}", build.display());
	for sub in [
		"pyrowave",
		"Granite/vulkan",
		"Granite/util",
		"Granite/math",
		"Granite/third_party",
	] {
		println!("cargo:rustc-link-search=native={}", build.join(sub).display());
	}
	for lib in [
		"pyrowave-capi",
		"pyrowave",
		"granite-vulkan",
		"granite-util",
		"granite-math",
		"granite-volk",
	] {
		println!("cargo:rustc-link-lib=static={lib}");
	}
	println!("cargo:rustc-link-lib=dylib=stdc++");
	// volk loads the Vulkan loader at runtime.
	println!("cargo:rustc-link-lib=dylib=dl");

	bindgen::Builder::default()
		.header("wrapper.h")
		.clang_arg(format!("-I{}", vendor.display()))
		.clang_arg(format!("-I{}", vk_include.display()))
		.allowlist_function("pyrowave_.*")
		.allowlist_type("pyrowave_.*")
		.allowlist_type("Vk(ImageDrmFormatModifierExplicitCreateInfoEXT|SubresourceLayout)")
		.allowlist_var("PYROWAVE_.*")
		.allowlist_var("VK_QUEUE_FAMILY_EXTERNAL")
		.derive_default(true)
		.generate()
		.expect("generate PyroWave bindings")
		.write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs"))
		.expect("write PyroWave bindings");
}

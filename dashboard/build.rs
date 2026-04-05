use cfg_aliases::cfg_aliases;

fn main() {
	// Rust 2024 edition: explicit check-cfg declarations
	println!("cargo::rustc-check-cfg=cfg(wasm)");
	println!("cargo::rustc-check-cfg=cfg(native)");

	cfg_aliases! {
		wasm: { target_arch = "wasm32" },
		native: { not(target_arch = "wasm32") },
	}
}

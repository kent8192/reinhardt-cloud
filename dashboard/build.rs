use cfg_aliases::cfg_aliases;

fn main() {
	// Rust 2024 edition: explicit check-cfg declarations
	println!("cargo::rustc-check-cfg=cfg(wasm)");
	println!("cargo::rustc-check-cfg=cfg(native)");
	println!("cargo::rustc-check-cfg=cfg(client)");
	println!("cargo::rustc-check-cfg=cfg(server)");

	cfg_aliases! {
		wasm: { target_arch = "wasm32" },
		native: { not(target_arch = "wasm32") },
		client: { any(target_arch = "wasm32", feature = "client-router") },
		server: { not(target_arch = "wasm32") },
	}
}

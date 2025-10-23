// build.rs - Configure getrandom for Solana BPF
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    
    // Get the target information
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    
    // For Solana BPF target, ensure custom getrandom is used
    if target_os == "solana" || target_arch == "bpf" {
        println!("cargo:rustc-cfg=getrandom_custom");
        println!("cargo:rustc-env=GETRANDOM_BACKEND=custom");
        println!("cargo:warning=Configuring custom getrandom for Solana BPF target");
    }
    
    // Always enable custom getrandom for this crate
    println!("cargo:rustc-cfg=getrandom_custom");
    println!("cargo:rustc-env=GETRANDOM_BACKEND=custom");
}
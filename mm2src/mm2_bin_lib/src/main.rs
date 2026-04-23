//! Native entry point for the Komodo DeFi Framework (Reloaded).

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        mm2::mm2_main()
    }
}

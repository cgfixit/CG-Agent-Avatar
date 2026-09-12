fn main() {
    #[cfg(target_os = "macos")]
    cg_agent::app::run();
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("cg-agent is macOS only");
        std::process::exit(2);
    }
}

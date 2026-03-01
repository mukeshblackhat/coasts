/// coast-dev — development-mode CLI that uses ~/.coast-dev/ and port 31416.
///
/// Identical to `coast` but targets the dev daemon so it never conflicts
/// with a globally-installed production coast.
fn main() -> anyhow::Result<()> {
    let home = dirs::home_dir().expect("could not determine home directory");
    unsafe {
        std::env::set_var("COAST_HOME", home.join(".coast-dev"));
        std::env::set_var("COAST_API_PORT", "31416");
        std::env::set_var("COAST_DNS_PORT", "5355");
    }

    tokio::runtime::Runtime::new()?.block_on(coast_cli::run())
}

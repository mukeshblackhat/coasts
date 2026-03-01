/// coastd-dev — development-mode daemon that uses ~/.coast-dev/ and port 31416.
///
/// Identical to `coastd` but targets the dev home directory so it never
/// conflicts with a globally-installed production coastd.
fn main() {
    let home = dirs::home_dir().expect("could not determine home directory");
    unsafe {
        std::env::set_var("COAST_HOME", home.join(".coast-dev"));
        std::env::set_var("COAST_API_PORT", "31416");
        std::env::set_var("COAST_DNS_PORT", "5355");
    }

    coast_daemon::run()
}

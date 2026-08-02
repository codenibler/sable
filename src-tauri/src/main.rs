fn main() {
    if std::env::args().any(|argument| argument == "--validate-config") {
        if let Err(error) = portfolio_1_lib::validate_config() {
            eprintln!("Sable configuration is invalid: {error}");
            std::process::exit(1);
        }
        println!("Sable configuration is valid.");
        return;
    }
    portfolio_1_lib::run();
}

use figment_clap_together::load_config;

fn main() {
    match load_config() {
        Ok(config) => println!("{config:#?}"),
        Err(e) => {
            eprintln!("failed to load config: {e}");
            std::process::exit(1);
        }
    }
}

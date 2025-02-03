use std::path::Path;

use clap::{Arg, ArgAction, Command};

pub fn config_cli() -> Command {
    Command::new("conf")
        .about("Dump the config from give path by using Rust `Debug` trait.")
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .value_name("Input config file")
                .action(ArgAction::Set)
                .required(true),
        )
}

pub fn list_config(file_path: &Path) {
    // let content = fs::read_to_string(file_path).unwrap();
    // let mut config: DionysiusConfig = toml::from_str(&content).unwrap();
    // let allow_modify = config.allow_modify.unwrap_or(false);
    // config.completion(allow_modify);
    println!("{}", crate::handlers::toml_config::load_config(file_path).unwrap());
}
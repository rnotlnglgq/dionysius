use super::{
    config::config_cli,
    push::*
};
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};

pub fn main_cli() -> Command {
    Command::new("Dionysius")
        .version("1.0")
        .author("Your Name <your.email@example.com>")
        .about("Assistant for file synchronization with git, borg, rsync etc..")
        // .subcommand(list_cli())
        .subcommand(config_cli())
        .subcommand(push_cli())
        .subcommand(test_cli())
        .arg(
            Arg::new("threads")
                .short('t')
                .long("threads")
                .value_name("Thread Number")
                .value_parser(value_parser!(usize))
                .help("Sets the maximum number of threads to use")
                .action(ArgAction::Set),
        )
}

pub fn push_cli() -> Command {
    Command::new("push")
        .about("Push to various backup targets")
        .arg(
            Arg::new("preview")
                .short('p')
                .long("preview")
                .help("Preview mode - show what would be done")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("execute")
                .short('e')
                .long("execute")
                .help("Enter execution mode, which performs real operations instead of print commands.")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("exclude")
                .short('x')
                .long("exclude")
                .value_name("PATTERN")
                .help("Exclude pattern to be added to the tasks")
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("search-hidden")
                .short('H')
                .long("search-hidden")
                .help("Go into directories whose name begins with `.`")
                .action(ArgAction::SetTrue)
        )
        .subcommand(push_git_cli())
        .subcommand(push_borg_cli())
        .subcommand(push_trigger_cli())
}

pub fn set_threads(matches: &ArgMatches) {
    // 设置线程池
    // if let Some(num_threads) = matches.get_one::<usize>("threads") {
    //     ThreadPoolBuilder::new()
    //         .num_threads(*num_threads)
    //         .build_global()
    //         .expect("Failed to build thread pool");
    // }
}

pub fn test_cli() -> Command {
    Command::new("test")
        .about("Test the function")
}
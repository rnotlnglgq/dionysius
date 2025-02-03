use std::path::Path;


mod cli;
mod handlers;
mod task;
mod log;

#[tokio::main]
async fn main() {
    let matches = cli::entry::main_cli().get_matches();

    match matches.subcommand() {
        Some(("test", _)) => {
            todo!()
        },
        Some(("push", sub_matches)) => {
            match sub_matches.subcommand() {
                Some(("trigger", sub2)) => cli::push::push_main(sub_matches, sub2, "trigger").await,
                Some(("git", sub2)) => cli::push::push_main(sub_matches, sub2, "git").await,
                Some(("borg", sub2)) => cli::push::push_main(sub_matches, sub2, "borg").await,
                _ => {
                    eprintln!("Unknown subcommand");
                    unreachable!()
                }
            }
        },
        // Some(("ls", sub_matches)) => {
        //     cli::entry::set_threads(&matches);
        //     cli::list::list_main(&sub_matches);
        // },
        Some(("conf", sub_matches)) => {
            let path = sub_matches.get_one::<String>("input").unwrap();
            // TODO: support -d option

            cli::config::list_config(&Path::new(&path));
        },
        None => {
            println!("Execute `dionysius help` to get help message.")
        },
        Some(s) => {
            eprintln!("{:?}", s);
            unreachable!()
        },
    }
}

use std::path::{absolute, PathBuf};
use clap::{Arg, ArgAction, ArgMatches, Command};
use crate::task::{self, CliTaskConfig, TaskList};

pub fn push_trigger_cli() -> Command {
    Command::new("trigger")
        .about("Let dionysius traverse directories and trigger backup tasks")
        .arg(
            Arg::new("directory")
                .short('d')
                .long("directory")
                .value_name("DIR")
                .help("Sets the root directory to traverse")
                .action(ArgAction::Set)
                .required(true)
        )
}

pub fn push_git_cli() -> Command {
    Command::new("git")
        .about("Push to remote git repositories")
        .arg(
            Arg::new("directory")
                .short('d')
                .long("directory")
                .value_name("DIR")
                .help("Sets the path of git repository to push")
                .action(ArgAction::Set)
                .required(true)
        )
}

pub fn push_borg_cli() -> Command {
    Command::new("borg")
        .about("Push to remote git repositories")
        .arg(
            Arg::new("directory")
                .short('d')
                .long("directory")
                .value_name("DIR")
                .help("Sets the path to archive by borg")
                .action(ArgAction::Set)
                .required(true)
        )
        .arg(
            Arg::new("execute")
                .short('e')
                .long("execute")
                .help("Enter execution mode, which performs real push instead of print commands.")
                .action(ArgAction::SetTrue)
        )
}

// TODO: add inheritation of trigger_by
pub async fn push_main(parent_matches: &ArgMatches, matches: &ArgMatches, task_type_id: &'static str) {
    let dir = matches.get_one::<String>("directory").unwrap();
    
    let search_hidden = parent_matches.get_flag("search-hidden");
    let cli_exclude_patterns: Vec<String> = parent_matches.get_many::<String>("exclude").unwrap_or_default().cloned().collect();
    let user_cli_config = CliTaskConfig {
        search_hidden,
        exclude_patterns: cli_exclude_patterns,
    };
    
    let execute_mode = parent_matches.get_flag("execute");
    let preview_mode = parent_matches.get_flag("preview");

    // 收集任务
    let task_list: TaskList = vec![];
    let task_list_ref = std::sync::Arc::new(std::sync::Mutex::new(task_list));
    
    task::collect_tasks(
        task_type_id,
        absolute(PathBuf::from(dir)).unwrap(),
        task_list_ref.clone(),
        None,
        None,
        user_cli_config
    ).await.inspect_err(|e| {
        eprintln!("{:?}", e);
    }).unwrap();

    let result = task_list_ref.lock().unwrap();
    
    if preview_mode {
        for task in result.iter() {
            task.preview().unwrap();
        }
    } else {
        let mut command_list = if execute_mode {
            None
        } else {
            Some(Vec::new())
        };
        
        for task in result.iter() {
            task.execute(&mut command_list);
        }

        if let Some(commands) = command_list {
            println!("{}", commands.join("\n"));
        }
    }
}

use std::{path::PathBuf, sync::{Arc, Mutex}};
use tokio;
use async_recursion::async_recursion;
use futures::future::join_all;
use walkdir::WalkDir;

use crate::{handlers::{
    borg::{BorgCreateOptions, BorgCreateTask}, exclude::{self, BorgPattern, GitIgnorePattern}, git::GitSaveTask, toml_config::{load_config, DionysiusConfig, HasInheritableConfig, OnRecursion, PushTaskConfig}, trigger::TriggerTask
}, log::{log, LogLevel}};

// *************************************************************************** //
// Types and traits
// *************************************************************************** //

pub trait PushTask where Self: std::fmt::Debug {
    // should return `Result` in future for handling errors.
	fn execute(&self, command_list: &mut Option<Vec<String>>);
	fn exclude_pattern_options(&self) -> Vec<String>;
    // Dev Note: pretend or preview?
    fn preview(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

pub type TaskList = Vec<Box<dyn PushTask + Send>>;

#[derive(Debug, Clone)]
pub struct CliTaskConfig {
    pub exclude_patterns: Vec<String>,
    pub search_hidden: bool,
}

// *************************************************************************** //
// Functions
// *************************************************************************** //

#[async_recursion]
pub async fn collect_tasks(
    task_type_id: &'static str,
    current_dir: PathBuf,
    task_list: Arc<Mutex<TaskList>>,
    super_config: Option<PushTaskConfig>,
    super_exclude_list: Option<Arc<Mutex<Vec<PathBuf>>>>,
    cli_config: CliTaskConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	// println!("current_dir: {:?}", current_dir);

    // ToDo: provide a flag, if yes, also skip dir matches `cli_exclude_patterns`

    // decouple `CliTaskConfig`
    let cli_exclude_patterns = &cli_config.exclude_patterns;
    let search_hidden = &cli_config.search_hidden;

    // Check if excluded
    if let Some(exclude_list) = super_exclude_list.as_ref() {
        let exclude_list = exclude_list.lock().unwrap();
        if exclude_list.contains(&current_dir.to_path_buf()) {
            return Ok(());
        }
    }
    if !search_hidden && current_dir.file_name().unwrap().to_string_lossy().starts_with(".") {
        return Ok(());
    }

    // Get configuration
    let config_path = current_dir.join("dionysius.toml");
    let config;
    let config_ref;
    let is_git_repo = current_dir.join(".git").is_dir();
    let config_exist = config_path.exists();

    // Early return for non-repo directories 
    if !config_exist && !is_git_repo {
        let mut subfolder_futures = Vec::new();
        
        for entry in WalkDir::new(&current_dir)
            .min_depth(1)
            .max_depth(1)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| e.file_type().is_dir()) {
                
            if let Ok(entry) = entry {
                let path = entry.path().to_path_buf();
                let future = tokio::spawn(collect_tasks(
                    task_type_id,
                    path,
                    task_list.clone(),
                    super_config.clone(),
                    super_exclude_list.clone(),
                    cli_config.clone(),
                ));
                subfolder_futures.push(future);
            }
        }

        let results = join_all(subfolder_futures).await;
        for res in results {
            res??;
        }
        return Ok(());
    }

	// println!("Repo found: {:?}", current_dir);

    // Get config
    if config_exist {
		// println!("Repo found: {:?}", current_dir);
        config = load_config(&config_path).unwrap();
		// should validate and set that bool correspondingly.
        config_ref = &config;
    } else {
        config_ref = DionysiusConfig::git_default_config();
    }


	// println!("{:?}", config_ref.push_task_configs());

    // Create exclude list for current directory
    let current_exclude_list_ref = Arc::new(Mutex::new(Vec::new()));

    // Process tasks based on config
    for (_child_task_type_id, push_config) in config_ref.push_task_configs().iter() {
		// use colored::Colorize;
		// println!("{}", format!("child_task_type_id: {:?}", child_task_type_id).red());
        let accepted_trigger = push_config.accepted_trigger();
        if accepted_trigger.contains(&task_type_id.to_string()) || task_type_id == "trigger" {
            use PushTaskConfig::*;
            let on_recursion: OnRecursion = match (push_config, &super_config) {
                (Trigger(trigger_config), _) => {
                    trigger_config.as_child.as_ref().unwrap().on_recursion.clone().expect("`on_recursion` must be manually set for trigger")
                },
                (_, None) => {
                    OnRecursion::Standalone
                },
                (Git(this_config), Some(Git(super_push_config))) => {
                    let merged = this_config.inherit_from(super_push_config);
                    merged.as_child.unwrap().on_recursion.unwrap()
                },
                (Borg(this_config), Some(Borg(super_push_config))) => {
                    let merged = this_config.inherit_from(super_push_config);
                    merged.as_child.unwrap().on_recursion.unwrap()
                },
                _ => {
                    unreachable!()
                }
            };
            match push_config {
				Git(this_config) => {
					// currently, current_exclude_list may be updated by multiple triggers.
                    let should_create_task = apply_recursion_strategy(
                        &current_dir,
                        &on_recursion,
                        super_exclude_list.clone()
                    )?;
                    if should_create_task {
						// process subdirectories: collect in subdirectories; update this exclude_list 
						process_subdirs(
							task_type_id,
							&current_dir,
							task_list.clone(),
							Some(push_config.clone()),
							current_exclude_list_ref.clone(),
                            cli_config.clone(),
						).await?;
						// reap the exclude_list
                        let exclude_list = current_exclude_list_ref.lock().unwrap().clone();
                        let extra_exclude_patterns: Vec<GitIgnorePattern> = cli_exclude_patterns.iter().filter_map(|str| {
                            GitIgnorePattern::try_from(str.clone()).map_err(|e| {
                                log(LogLevel::Warn, e);
                            }).ok()
                        }).collect();
                        // create and append the task
						let task = GitSaveTask {
                            repo_path: current_dir.clone(),
                            exclude_list,
                            unsaved_behavior: this_config.as_child.as_ref().unwrap().on_unsave.as_ref().unwrap().clone(),
                            extra_exclude_patterns: extra_exclude_patterns,
                        };
                        task_list.lock().unwrap().push(Box::new(task));
                    } else {
						return Ok(())
					}
                },
				Borg(this_config) => {
					// currently, current_exclude_list may be updated by multiple triggers.
                    let should_create_task = apply_recursion_strategy(
                        &current_dir,
                        &on_recursion,
                        super_exclude_list.clone()
                    )?;
                    if should_create_task {
						// process subdirectories: collect in subdirectories; update this exclude_list 
						process_subdirs(
							task_type_id,
							&current_dir,
							task_list.clone(),
							Some(push_config.clone()),
							current_exclude_list_ref.clone(),
                            cli_config.clone(),
						).await?;
						// reap the exclude_list
                        let exclude_list = current_exclude_list_ref.lock().unwrap().clone();
                        let mut extra_exclude_patterns: Vec<BorgPattern> = cli_exclude_patterns.iter().filter_map(|str| {
                            BorgPattern::try_from(str.clone()).map_err(|e| {
                                log(LogLevel::Warn, e);
                            }).ok()
                        }).collect();
                        if let Some(config_exclude_patterns) = this_config.as_child.as_ref().unwrap().exclude_list.clone() {
                            extra_exclude_patterns.extend(
                                config_exclude_patterns.iter().filter_map(|str| {
                                    BorgPattern::try_from(str.clone()).map_err(|e| {
                                        log(LogLevel::Warn, e);
                                    }).ok()
                                })
                            );
                        }
                        if let Some(extra_exclude_modes) = &this_config.as_child.as_ref().unwrap().extra_exclude_mode {
                            let gitignore_path = &current_dir.join(".gitignore");
                            if extra_exclude_modes.contains(&"git".to_string()) && gitignore_path.is_file() {
                                let patterns = crate::handlers::exclude::read_gitignore(gitignore_path);
                                patterns.into_iter().filter_map(|p| {
                                    BorgPattern::try_from(p).map_err(|e| {
                                        log(LogLevel::Warn, e);
                                    }).ok()
                                }).for_each(|pattern| {
                                    extra_exclude_patterns.push(pattern);
                                });
                            }
                        }
                        // create and append the task
						let task = BorgCreateTask {
							source: current_dir.clone(),
							target: {
								match &config_ref.borg {
									Some(PushTaskConfig::Borg(borg_conf)) => {
										borg_conf.target.as_ref().unwrap().target.clone().unwrap()
									},
									_ => panic!(),
								}
							},
							exclude_list,
                            extra_exclude_patterns,
							options: BorgCreateOptions::default()
                        };
                        task_list.lock().unwrap().push(Box::new(task));
                    } else {
						return Ok(())
					}
                },
                // TODO: allow this to provide config advise as a super.
				Trigger(this_config) => {
                    let on_recursion = this_config.as_child.as_ref().unwrap().on_recursion.clone().unwrap();
					// currently, current_exclude_list may be updated by multiple triggers.
                    let should_create_task = apply_recursion_strategy(
                        &current_dir,
                        &on_recursion,
                        super_exclude_list.clone()
                    )?;
                    if should_create_task {
						// process subdirectories: collect in subdirectories; update super exclude_list 
						process_subdirs(
							task_type_id,
							&current_dir,
							task_list.clone(),
							Some(push_config.clone()),
							super_exclude_list.clone().unwrap(),
                            cli_config.clone(),
						).await?;
                        // create and append the task
						let task = TriggerTask {current_dir: current_dir.clone()};
                        task_list.lock().unwrap().push(Box::new(task));
                    } else {
						return Ok(())
					}
                },
				_ => {}
            }
        }
    }

    Ok(())
}

// TODO: can use this for the trivial subdir case, too
async fn process_subdirs(
    task_type_id: &'static str,
    current_dir: &PathBuf,
    task_list: Arc<Mutex<TaskList>>,
    super_config: Option<PushTaskConfig>,
    current_exclude_list_ref: Arc<Mutex<Vec<PathBuf>>>,
    cli_config: CliTaskConfig, // 新增参数
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut subfolder_futures = Vec::new();
    
    // Use synchronous walkdir to avoid tokio::fs::read_dir() open too many files
    for entry in WalkDir::new(current_dir)
        .min_depth(1)
        .max_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_type().is_dir()) {
            
        if let Ok(entry) = entry {
            let path = entry.path().to_path_buf();
            let future = tokio::spawn(collect_tasks(
                task_type_id,
                path,
                task_list.clone(),
                super_config.clone(), 
                Some(current_exclude_list_ref.clone()),
                cli_config.clone(),
            ));
            subfolder_futures.push(future);
        }
    }

    let results = join_all(subfolder_futures).await;
    for res in results {
        res??;
    }

    Ok(())
}

fn apply_recursion_strategy(
	current_dir: &PathBuf,
	on_recursion: &OnRecursion,
	super_exclude_list: Option<Arc<Mutex<Vec<PathBuf>>>>
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
	match on_recursion {
		OnRecursion::Skip => {
            super_exclude_list.map(|list_ref| {
                list_ref.lock().unwrap().push(current_dir.clone());
            });
            Ok(false)
		},
		OnRecursion::Include => {
            // regarded as not a submodule
            Ok(false)
		},
		OnRecursion::Standalone => {
            super_exclude_list.map(|list_ref| {
                list_ref.lock().unwrap().push(current_dir.clone());
            });
            // collect this submodule as a task
            Ok(true)
		},
		OnRecursion::Double => {
            // collect this submodule as a task
            Ok(true)
		},
		OnRecursion::Inherit => {
            unreachable!()
		},
	}
}
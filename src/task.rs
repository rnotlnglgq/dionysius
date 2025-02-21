use core::panic;
use std::{path::PathBuf, sync::{Arc, Mutex}};
use tokio;
use async_recursion::async_recursion;
use futures::future::join_all;
use walkdir::WalkDir;

use crate::{
    handlers::{
        borg::{BorgCreateOptions, BorgCreateTask},
        exclude::{BorgPattern, GitIgnorePattern},
        git::GitSaveTask,
        toml_config::{load_config, DionysiusConfig, HasInheritableConfig, OnRecursion, PushTaskConfig},
        trigger::TriggerTask
    }, log::{log, LogLevel}
};

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

fn inherit_config(
    this_config: &DionysiusConfig,
    super_config: Option<&DionysiusConfig>
) -> DionysiusConfig {
    let mut config_clone = this_config.clone();
    
    if let Some(super_config) = super_config {
        for (field_name, push_config) in this_config.push_task_configs().iter() {
            use PushTaskConfig::*;
            match push_config {
                Git(this_config) => {
                    let super_push_config_inner = super_config
                        .git
                        .as_ref()
                        .map(|c| c.get_git().unwrap());
                    let merged = this_config.inherit_from(super_push_config_inner);
                    config_clone.map_at_push_task_configs_mut(
                        |field_name_opt| field_name_opt == Some(field_name),
                        |_| Git(merged.clone())
                    );
                },
                Borg(this_config) => {
                    let super_push_config_inner = super_config 
                        .borg
                        .as_ref()
                        .map(|c| c.get_borg().unwrap());
                    let merged = this_config.inherit_from(super_push_config_inner);
                    config_clone.map_at_push_task_configs_mut(
                        |field_name_opt| field_name_opt == Some(field_name),
                        |_| Borg(merged.clone())
                    );
                },
                _ => {
                    log(LogLevel::Error, format!("Unimplemented inheritance for {:?}", push_config).as_str());
                    unreachable!()
                }
            }
        }
    } else {
        // Inherit from default values when no super config
        for (field_name, push_config) in this_config.push_task_configs().iter() {
            use PushTaskConfig::*;
            match push_config {
                Git(this_config) => {
                    let merged = this_config.inherit_from(None);
                    config_clone.map_at_push_task_configs_mut(
                        |field_name_opt| field_name_opt == Some(field_name),
                        |_| Git(merged.clone())
                    );
                },
                Borg(this_config) => {
                    let merged = this_config.inherit_from(None);
                    config_clone.map_at_push_task_configs_mut(
                        |field_name_opt| field_name_opt == Some(field_name),
                        |_| Borg(merged.clone())
                    );
                },
                _ => {}
            }
        }
    }

    config_clone
}

#[async_recursion]
pub async fn collect_tasks(
    task_type_id: &'static str,
    current_dir: PathBuf,
    task_list: Arc<Mutex<TaskList>>,
    super_config: Option<DionysiusConfig>,
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
    if !search_hidden {
        if let Some(name) = current_dir.file_name() {
            if name.to_string_lossy().starts_with(".") {
                return Ok(());
            }
        }
    }

    // Get configuration
    let config_path = current_dir.join("dionysius.toml");
    let mut config;
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

    // Inherit config
    let config_clone = inherit_config(config_ref, super_config.as_ref());
    let config_ref = &config_clone;

	// println!("{:?}", config_ref.push_task_configs());

    // Create exclude list for current directory
    let current_exclude_list_ref = Arc::new(Mutex::new(Vec::new()));

    // Process tasks based on config
    // let mut config_clone = config_ref.clone();
    for (_child_task_type_id, push_config) in config_ref.push_task_configs().iter() {
        let accepted_trigger = push_config.accepted_trigger();
        if !accepted_trigger.contains(&task_type_id.to_string()) && task_type_id != "trigger" {
            break;
        }
        // inherit config
        use PushTaskConfig::*;
        // if let Some(ref super_config_unwrapped) = super_config {
        //     match push_config {
        //         Trigger(trigger_config) => {
        //             on_recursion = trigger_config.assets.as_ref().unwrap().on_recursion.clone().expect("`on_recursion` must be manually set for trigger")
        //         },
        //         Git(this_config) => {
        //             let super_push_config_inner = super_config_unwrapped
        //                 .git
        //                 .as_ref()
        //                 .map(|c| {
        //                     c.get_git().unwrap()
        //                 });
        //             let merged = this_config.inherit_from(super_push_config_inner);
        //             on_recursion = merged.assets.as_ref().unwrap().on_recursion.clone().unwrap()
        //             ;
        //             config_clone.git = Some(Git(merged));
        //         },
        //         Borg(this_config) => {
        //             let super_push_config_inner = super_config_unwrapped
        //                 .borg
        //                 .as_ref()
        //                 .map(|c| {
        //                     c.get_borg().unwrap()
        //                 });
        //             let merged = this_config.inherit_from(super_push_config_inner);
        //             on_recursion = merged.assets.as_ref().unwrap().on_recursion.clone().unwrap()
        //             ;
        //             config_clone.git = Some(Borg(merged));
        //         },
        //         _ => {
        //             log(LogLevel::Error, format!("Unimplemented inheritance for {:?}; {:?}", push_config, super_config).as_str());
        //             unreachable!()
        //         }
        //     };
        // } else {
        //     on_recursion = OnRecursion::default();
        // };
        let on_recursion: OnRecursion = match push_config {
            Trigger(trigger_config) => {
                trigger_config.assets.as_ref().unwrap().on_recursion.clone().expect("`on_recursion` must be manually set for trigger")
            },
            Git(this_config) => {
                this_config.assets.as_ref().unwrap().on_recursion.clone().unwrap()
            },
            Borg(this_config) => {
                this_config.assets.as_ref().unwrap().on_recursion.clone().unwrap()
            },
            _ => {
                log(LogLevel::Error, format!("Unimplemented inheritance for {:?}; {:?}", push_config, super_config).as_str());
                unreachable!()
            }
        };
        // log(LogLevel::Info, format!("{:?}", current_dir).as_str());
        // log(LogLevel::Info, format!("{:?}", super_config).as_str());
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
                        Some(config_ref.clone()),
                        current_exclude_list_ref.clone(),
                        cli_config.clone(),
                    ).await?;
                    // reap the exclude_list
                    let exclude_list = current_exclude_list_ref.lock().unwrap().clone();
                    let extra_exclude_patterns: Vec<GitIgnorePattern> = cli_exclude_patterns.iter().filter_map(|str| {
                        GitIgnorePattern::try_from(str.clone()).inspect_err(|e| {
                            log(LogLevel::Warn, e);
                        }).ok()
                    }).collect();
                    // create and append the task
                    let task = GitSaveTask {
                        repo_path: current_dir.clone(),
                        exclude_list,
                        unsaved_behavior: this_config.assets.as_ref().unwrap().on_unsave.as_ref().unwrap().clone(),
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
                        Some(config_ref.clone()),
                        current_exclude_list_ref.clone(),
                        cli_config.clone(),
                    ).await?;
                    // reap the exclude_list
                    let exclude_list = current_exclude_list_ref.lock().unwrap().clone();
                    let mut extra_exclude_patterns: Vec<BorgPattern> = cli_exclude_patterns.iter().filter_map(|str| {
                        BorgPattern::try_from(str.clone()).inspect_err(|e| {
                            log(LogLevel::Warn, e);
                        }).ok()
                    }).collect();
                    if let Some(config_exclude_patterns) = this_config.assets.as_ref().unwrap().exclude_list.clone() {
                        extra_exclude_patterns.extend(
                            config_exclude_patterns.iter().filter_map(|str| {
                                BorgPattern::try_from(str.clone()).inspect_err(|e| {
                                    log(LogLevel::Warn, e);
                                }).ok()
                            })
                        );
                    }
                    if let Some(extra_exclude_modes) = &this_config.assets.as_ref().unwrap().extra_exclude_mode {
                        let gitignore_path = &current_dir.join(".gitignore");
                        if extra_exclude_modes.contains(&"git".to_string()) && gitignore_path.is_file() {
                            let patterns = crate::handlers::exclude::read_gitignore(gitignore_path);
                            patterns.into_iter().filter_map(|p| {
                                BorgPattern::try_from(p).inspect_err(|e| {
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
                let on_recursion = this_config.assets.as_ref().unwrap().on_recursion.clone().unwrap();
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
                        Some(config_ref.clone()),
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

    Ok(())
}

// TODO: can use this for the trivial subdir case, too
async fn process_subdirs(
    task_type_id: &'static str,
    current_dir: &PathBuf,
    task_list: Arc<Mutex<TaskList>>,
    super_config: Option<DionysiusConfig>,
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
            log(LogLevel::Error, format!("Unexpected `on_recursion=inherit` in {:?}", current_dir).as_str());
            unreachable!()
		},
	}
}
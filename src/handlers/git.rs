use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::fmt;
use std::error::Error as StdError;
use std::sync::OnceLock;
use bevy_reflect::Reflect;
use git2::Repository;
use git2::Error as LibGitError;
use colored::*;
use serde::Deserialize;

use crate::handlers::toml_config::PushTaskConfig;
use crate::log::{log, LogLevel};
use crate::task::PushTask;
use super::exclude::GitIgnorePattern;
use super::toml_config::{CompletableConfig, DionysiusConfig, HasInheritableConfig, InheritableConfig, OnRecursion};

// *************************************************************************** //
// Configuration Types and Implementations
// *************************************************************************** //

#[derive(Debug, Deserialize, Clone, Reflect)]
pub struct GitTargetConfig {
    pub mode: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Reflect)]
pub struct GitConfig {
    pub target: Option<GitTargetConfig>,
    pub assets: Option<GitInheritableConfig>,
    pub heritage: Option<GitInheritableConfig>,
}

#[derive(Debug, Deserialize, Clone, Reflect)]
pub struct GitInheritableConfig {
    pub trigger_by: Option<Vec<String>>,
    pub on_unsave: Option<OnUnsave>,
    pub on_recursion: Option<OnRecursion>,
    pub ignore_child: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Reflect)]
pub enum OnUnsave {
    #[serde(rename = "save")]
    Save,
    #[serde(rename = "ignore")]
    Ignore,
    #[serde(rename = "ask")]
    Ask,
    #[serde(rename = "interrupt")]
    Interrupt,
}

// *************************************************************************** //
// Task Types and Implementations
// *************************************************************************** //

#[derive(Debug)]
pub struct GitSaveTask {
    pub repo_path: PathBuf,
    pub exclude_list: Vec<PathBuf>,
    pub unsaved_behavior: OnUnsave,
    pub extra_exclude_patterns: Vec<GitIgnorePattern>,
}

impl PushTask for GitSaveTask {
    fn execute(&self, command_list: &mut Option<Vec<String>>) {
        let _ =
        autosave_and_push(self, command_list)
            .map_err(|err| eprintln!("{:?}", err));
            // .expect("Failed to autosave and push");
    }

    fn exclude_pattern_options(&self) -> Vec<String> {
        // 对每个需要排除的路径生成 :(exclude) pathspec
        // 参考: https://git-scm.com/docs/gitglossary#Documentation/gitglossary.txt-aiddefpathspecapathspec
        let mut patterns = Vec::new();
        let base_path = &self.repo_path;
        self.exclude_list.iter()
            .filter_map(|exclude_path| {
                exclude_path.strip_prefix(base_path).ok()
                    .map(|relative| {
                        format!(":(exclude){}", 
                            relative.to_string_lossy().replace('\\', "/")
                        )
                    })
            })
            .for_each(|pattern| {
                patterns.push(pattern)
            });
        self.extra_exclude_patterns
            .iter()
            .map(|pattern| pattern.pattern.clone())
            .for_each(|pattern| {
                patterns.push(pattern)
            });
        patterns
    }

    fn preview(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let metadata = git_repo_metadata(&self.repo_path).map_err(|e| {
            println!("{:?}", self.repo_path);
            e
        })?;
        println!(
            "Git: [{}] [{}] file://{}", 
            metadata.repo_check.to_string(),
            metadata.work_status.to_string(),
            self.repo_path.canonicalize()?.display()
        );
        Ok(())
    }
}

// *************************************************************************** //
// Error Types
// *************************************************************************** //

#[derive(Debug)]
pub enum GitError {
    LibGitError(LibGitError),
    GitCommandError(GitCommandError),
}

#[derive(Debug)]
pub struct GitCommandError {
    pub message: String,
}

impl From<LibGitError> for GitError {
    fn from(err: LibGitError) -> GitError {
        GitError::LibGitError(err)
    }
}

impl From<GitCommandError> for GitError {
    fn from(err: GitCommandError) -> GitError {
        GitError::GitCommandError(err)
    }

}

impl fmt::Display for GitCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl StdError for GitCommandError {}

// *************************************************************************** //
// Repository Status Types
// *************************************************************************** //

pub struct RepoWorkStatus {
    pub workdir_unsaved: Option<bool>,
    pub index_unsaved: Option<bool>,
    pub ahead_upstream: Option<bool>,
    pub behind_upstream: Option<bool>,
    pub submodule_exist: Option<bool>,
    pub diverged: Option<bool>,
}

impl RepoWorkStatus {
    pub fn to_string(&self) -> String {
        let mut flags = Vec::new();

        flags.push(match self.workdir_unsaved {
            Some(true) => "W".yellow().to_string(),
            Some(false) => " ".to_string(),
            None => "/".red().to_string(),
        });
        flags.push(match self.index_unsaved {
            Some(true) => "I".yellow().to_string(),
            Some(false) => " ".to_string(),
            None => "/".red().to_string(),
        });
        flags.push(match self.ahead_upstream {
            Some(true) => {
                if self.diverged.unwrap_or(false) { "A".red().to_string() } else { "A".green().to_string() }
            },
            Some(false) => " ".to_string(),
            None => "/".red().to_string(),
        });
        flags.push(match self.behind_upstream {
            Some(true) => {
                if self.diverged.unwrap_or(false) { "B".red().to_string() } else { "B".yellow().to_string() }
            },
            Some(false) => " ".to_string(),
            None => "/".red().to_string(),
        });
        flags.push(match self.submodule_exist {
            Some(true) => "S".yellow().to_string(),
            Some(false) => " ".to_string(),
            None => "/".red().to_string(),
        });

        flags.concat()
    }
}

pub struct GitRepoMetaData {
    worktree: PathBuf,
    gitdir: PathBuf,
    repo_check: RepoCheck,
    work_status: RepoWorkStatus,
}

pub enum BranchCount {
    Zero,
    One,
    Many,
}

pub struct RepoCheck {
    pub is_bare: Option<bool>,
    pub head_points_to_branch: Option<bool>,
    pub branch_exists: Option<bool>,
    pub local_branch_count: Option<BranchCount>,
    pub remote_branch_count: Option<BranchCount>,
    pub has_upstream: Option<bool>,
}

impl RepoCheck {
    pub fn to_string(&self) -> String {
        let mut flags = Vec::new();

        flags.push(match self.is_bare {
            Some(true) => "B".red().to_string(),
            Some(false) => " ".to_string(),
            None => "/".red().to_string(),
        });
        flags.push(match self.head_points_to_branch {
            Some(true) => "H".green().to_string(),
            Some(false) => " ".to_string(),
            None => "/".red().to_string(),
        });
        flags.push(match self.branch_exists {
            Some(true) => "E".green().to_string(),
            Some(false) => " ".to_string(),
            None => "/".red().to_string(),
        });
        flags.push(match self.local_branch_count {
            Some(BranchCount::Zero) => "0".red().to_string(),
            Some(BranchCount::One) => "1".green().to_string(),
            Some(BranchCount::Many) => "M".yellow().to_string(),
            None => "/".red().to_string(),
        });
        flags.push(match self.remote_branch_count {
            Some(BranchCount::Zero) => "0".red().to_string(),
            Some(BranchCount::One) => "1".green().to_string(),
            Some(BranchCount::Many) => "M".yellow().to_string(),
            None => "/".red().to_string(),
        });
        flags.push(match self.has_upstream {
            Some(true) => "U".green().to_string(),
            Some(false) => " ".to_string(),
            None => "/".red().to_string(),
        });

        flags.concat()
    }
}

// *************************************************************************** //
// Configuration Trait Implementations
// *************************************************************************** //

impl HasInheritableConfig for GitConfig {
    type M = GitInheritableConfig;
    
    // fn inherit_from(&self, super_config: &Option<Self>) -> Self {
    //     let mut this = self.clone();
    //     if let Some(super_config) = super_config {
    //         this.assets = Some(
    //             this.assets
    //                 .unwrap()
    //                 .inherit_from(
    //                     super_config.get_heritage_config()
    //                 )
    //         );
    //         this.heritage = Some(
    //             this.heritage.unwrap()
    //                 .inherit_from(super_config.get_heritage_config())
    //         );
    //         this
    //     } else {
    //         this
    //     }
    // }
    fn get_assets_config(&self) -> &GitInheritableConfig {
        self.assets.as_ref().unwrap()
    }
    fn get_heritage_config(&self) -> &GitInheritableConfig {
        self.heritage.as_ref().unwrap()
    }
    fn get_assets_config_mut(&mut self) -> &mut Self::M {
        self.assets.as_mut().unwrap()
    }
    fn get_heritage_config_mut(&mut self) -> &mut Self::M {
        self.heritage.as_mut().unwrap()
    }
}

impl InheritableConfig for GitInheritableConfig {
    fn inherit_from(&self, super_config: Option<&Self>) -> Self {
        let mut this = self.clone();
        match self.on_recursion {
            Some(OnRecursion::Inherit) => {
                match super_config {
                    Some(super_config) => {
                        this.on_recursion = super_config.on_recursion.clone();
                    },
                    None => {
                        this.on_recursion = Some(OnRecursion::default());
                    }
                }
            },
            None => unreachable!(),
            _ => {}
        }
        this
    }
}

macro_rules! check_fields {
    ($obj:expr, $($field:ident),+) => {
        {
            let mut complete = true;
            $(
                if $obj.$field.is_none() {
                    complete = false;
                }
            )+
            complete
        }
    };
}

impl CompletableConfig for GitConfig {
    type CompletionResult = Result<Self, &'static str>;

    fn is_complete(&self) -> bool {
        // todo: check target
        if let Some(as_child) = &self.assets {
            if !check_fields!(as_child, trigger_by, on_unsave, on_recursion) {
                return false;
            }
        } else {
            return false;
        }
        if let Some(as_super) = &self.heritage {
            if !check_fields!(as_super, ignore_child, on_recursion) {
                return false;
            }
        } else {
            return false;
        }
        true
    }

    fn completion(&self) -> Self::CompletionResult {
        let mut result = self.clone();
        let default = GitConfig::default();

        // check target
        if let Some(target) = &self.target {
            if target.target.is_none() {
                return Err("Target string cannot be empty");
            }
            if !["gitconfig", "path"].contains(&target.mode.as_ref().unwrap().as_str()) {
                return Err("Invalid target mode");
            }
        }

        // complete as_child
        if let Some(as_child) = &mut result.assets {
            if as_child.trigger_by.is_none() {
                as_child.trigger_by = default.assets.as_ref().unwrap().trigger_by.clone();
            }
            if as_child.on_recursion.is_none() {
                as_child.on_recursion = default.assets.as_ref().unwrap().on_recursion.clone();
            }
            if as_child.on_unsave.is_none() {
                as_child.on_unsave = default.assets.as_ref().unwrap().on_unsave.clone();
            }
        } else {
            result.assets = default.assets;
        }

        // complete as_super 
        if let Some(as_super) = &mut result.heritage {
            if as_super.ignore_child.is_none() {
                as_super.ignore_child = default.heritage.as_ref().unwrap().ignore_child;
            }
            if as_super.on_recursion.is_none() {
                as_super.on_recursion = default.heritage.as_ref().unwrap().on_recursion.clone();
            }
        } else {
            result.heritage = default.heritage;
        }

        Ok(result)
    }
}

// *************************************************************************** //
// Default Implementations
// *************************************************************************** //

impl Default for OnUnsave {
    fn default() -> Self {
        OnUnsave::Ask
    }
}

impl Default for GitConfig {
    fn default() -> Self {
        GitConfig {
            target: Some(GitTargetConfig {
                mode: "gitconfig".to_string().into(),
                target: "".to_string().into(),
            }),
            assets: Some(GitInheritableConfig {
                ignore_child: None,
                trigger_by: Some(vec!["git".to_string(), "borg".to_string()]),
                on_unsave: Some(OnUnsave::Save),
                on_recursion: Some(OnRecursion::Inherit),
            }),
            heritage: Some(GitInheritableConfig {
                ignore_child: Some(false),
                trigger_by: None,
                on_unsave: Some(OnUnsave::Save),
                on_recursion: Some(OnRecursion::Inherit),

            }),
        }
    }
}

// *************************************************************************** //
// Display Implementations
// *************************************************************************** //

impl fmt::Display for GitConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "GitConfig:")?;
        if let Some(target) = &self.target {
            writeln!(f, "  Target Mode: {:?}", target.mode)?;
            writeln!(f, "  Target: {:?}", target.target)?;
        }
        if let Some(as_child) = &self.assets {
            writeln!(f, "  As Child:")?;
            writeln!(f, "    Trigger By: {:?}", as_child.trigger_by)?;
            writeln!(f, "    On Unsave: {:?}", as_child.on_unsave)?;
            writeln!(f, "    On Recursion: {:?}", as_child.on_recursion)?;
            writeln!(f, "    Ignore Child: {:?}", as_child.ignore_child)?;
        }
        if let Some(as_super) = &self.heritage {
            writeln!(f, "  As Super:")?;
            writeln!(f, "    Trigger By: {:?}", as_super.trigger_by)?;
            writeln!(f, "    On Unsave: {:?}", as_super.on_unsave)?;
            writeln!(f, "    On Recursion: {:?}", as_super.on_recursion)?;
            writeln!(f, "    Ignore Child: {:?}", as_super.ignore_child)?;
        }
        Ok(())
    }
}

// *************************************************************************** //
// Repository Operations
// *************************************************************************** //

pub fn autosave_and_push(
    task: &GitSaveTask,
    command_list: &mut Option<Vec<String>>,
) -> Result<(), GitError> {
    let repo = Repository::open(&task.repo_path)?;

    if !is_tree_clean(&repo)? {
        match &task.unsaved_behavior {
            OnUnsave::Save => {
                add_to_index(&repo, &task.exclude_list, command_list)?;
                log(LogLevel::Info, "Update index with workdir.");
            },
            OnUnsave::Ignore => {
                log(LogLevel::Warn, "Working directory is not clean.");
            },
            OnUnsave::Ask => {
                log(LogLevel::Info, "Do you want to save changes to index? [Y/n]");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).expect("Failed to read input");
                if input.trim().to_lowercase() == "y" || input.trim().is_empty() {
                    add_to_index(&repo, &task.exclude_list, command_list)?;
                    log(LogLevel::Info, "Update index with workdir.");
                }
            },
            OnUnsave::Interrupt => {
                return Err(GitCommandError { message: "Working directory is not clean.".to_string() }.into());
            },
        }
    }

    if !is_index_clean(&repo)? {
        match &task.unsaved_behavior {
            OnUnsave::Save => {
                commit_to_head(&repo, "Autosave by dionysius", command_list)?;
                log(LogLevel::Info, "Commit current index.");
            },
            OnUnsave::Ignore => {
                log(LogLevel::Warn, "Index is not clean.");
            },
            OnUnsave::Ask => {
                log(LogLevel::Info, "Do you want to commit changes to head? [Y/n]");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).expect("Failed to read input");
                if input.trim().to_lowercase() == "y" || input.trim().is_empty() {
                    commit_to_head(&repo, "Autosave by dionysius", command_list)?;
                    log(LogLevel::Info, "Commit current index.");
                }
            },
            OnUnsave::Interrupt => {
                return Err(GitCommandError { message: "Index is not clean.".to_string() }.into());
            },
        }
    }

    fetch_upstream(repo.workdir().expect("There is no workdir."), command_list)?;
    
    let (ahead, behind) = upstream_status(&repo)?;
    if ahead && behind {
        log(LogLevel::Error, "Repository has diverged from upstream.");
    } else if ahead {
        push_upstream(repo.workdir().expect("There is no workdir."), command_list)?;
    } else if behind {
        log(LogLevel::Warn, "You can pull from the upstream.");
    } else {
        log(LogLevel::Info, "Repository is already up to date.");
    }

    Ok(())
}

pub fn push_if_saved(repo: &Repository, command_list: &mut Option<Vec<String>>) -> Result<(), GitError> {
    if !is_tree_clean(repo)? {
        log(LogLevel::Warn, "Working directory is not clean.");
        // return Ok(());
    }

    if !is_index_clean(repo)? {
        log(LogLevel::Warn, "Index is not clean.");
        // return Ok(());
    }

    fetch_upstream(repo.workdir().expect("There is no workdir."), command_list)?;
    
    let (ahead, behind) = upstream_status(repo)?;
    if ahead && behind {
        log(LogLevel::Error, "Repository has diverged from upstream.");
    } else if ahead {
        push_upstream(repo.workdir().expect("There is no workdir."), command_list)?;
    } else if behind {
        log(LogLevel::Warn, "You can pull from the upstream.");
    } else {
        log(LogLevel::Info, "Repository is already up to date.");
    }

    Ok(())
}

pub fn push_upstream(repo_path: &Path, command_list: &mut Option<Vec<String>>) -> Result<(), GitError> {
    let mut command = std::process::Command::new("git");
    command
        .arg("-C")
        .arg(repo_path)
        .arg("push");

    if let Some(list) = command_list {
        list.push(format!("{:?}", command));
        Ok(())
    } else {
        let output = command.output().expect("Failed to execute git push");

        if output.status.success() {
            log(LogLevel::Info, "Successfully pushed to upstream.");
            Ok(())
        } else {
            let stderr_cow = String::from_utf8_lossy(&output.stderr);
            log(LogLevel::Error, &format!("Failed to push to upstream: {}", stderr_cow));
            Err(GitCommandError { message: stderr_cow.to_string() }.into())
        }
    }
}

pub fn fetch_upstream(repo_path: &Path, command_list: &mut Option<Vec<String>>) -> Result<(), GitError> {
    let mut command = std::process::Command::new("git");
    command
        .arg("-C")
        .arg(repo_path)
        .arg("fetch");

    if let Some(list) = command_list {
        list.push(format!("{:?}", command));
        Ok(())
    } else {
        let output = command.output().expect("Failed to execute git fetch");

        if output.status.success() {
            log(LogLevel::Info, "Successfully fetched from upstream.");
            Ok(())
        } else {
            let stderr_cow = String::from_utf8_lossy(&output.stderr);
            log(LogLevel::Error, &format!("Failed to fetch from upstream: {}", stderr_cow));
            Err(GitCommandError { message: stderr_cow.to_string() }.into())
        }
    }
}

pub fn add_to_index(
    repo: &Repository, 
    exclude_list: &[PathBuf],
    command_list: &mut Option<Vec<String>>,
) -> Result<(), GitError> {
    if let Some(list) = command_list {
        // shell mode
        let mut command = std::process::Command::new("git");
        command
            .arg("-C")
            .arg(repo.workdir().expect("There is no workdir."))
            .arg("add")
            .arg(".");

        let base_path = repo.workdir().expect("There is no workdir.");
        for exclude_path in exclude_list {
            if let Ok(relative) = exclude_path.strip_prefix(base_path) {
                let pattern = format!(
                    ":(exclude){}", 
                    relative.to_string_lossy().replace('\\', "/")
                );
                command.arg(&pattern);
            }
        }

        list.push(format!("{:?}", command));
        Ok(())
    } else {
        // libgit2 mode
        let mut index = repo.index()?;
        let exclude_list = exclude_list.to_vec();
        
        index.add_all(
            ["*"].iter(), 
            git2::IndexAddOption::DEFAULT,
            Some(&mut |path: &Path, _matched_spec: &[u8]| -> i32 {
                if exclude_list.contains(&path.to_path_buf()) {1} else {0}
            }),
        )?;
        
        index.write()?;
        log(LogLevel::Info, "Successfully added to index.");
        Ok(())
    }
}

pub fn commit_to_head(repo: &Repository, message: &str, command_list: &mut Option<Vec<String>>) -> Result<(), GitError> {
    if let Some(list) = command_list {
        let mut command = std::process::Command::new("git");
        command
            .arg("-C")
            .arg(repo.workdir().expect("There is no workdir."))
            .arg("commit")
            .arg("-m")
            .arg(message);
        list.push(format!("{:?}", command));
        Ok(())
    } else {
        let mut index = repo.index()?;
        let oid = index.write_tree()?;
        let signature = repo.signature()?;
        let parent_commit = repo.head()?.peel_to_commit()?;
        let tree = repo.find_tree(oid)?;
        repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[&parent_commit])?;
        log(LogLevel::Info, "Successfully committed to head.");
        Ok(())
    }
}

pub fn is_tree_clean(repo: &Repository) -> Result<bool, GitError> {
    let diff = repo.diff_index_to_workdir(None, None)?;
    Ok(diff.deltas().count() == 0)
}

pub fn is_index_clean(repo: &Repository) -> Result<bool, GitError> {
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    let diff = repo.diff_tree_to_index(Some(&tree), None, None)?;
    Ok(diff.deltas().count() == 0)
}

pub fn upstream_status(repo: &Repository) -> Result<(bool, bool), GitError> {
    let head = repo.head()?;
    let branch = head.shorthand().ok_or_else(|| git2::Error::from_str("No branch found"))?;
    let upstream = repo.find_branch(branch, git2::BranchType::Local)?.upstream()?;
    let upstream_commit = upstream.get().peel_to_commit()?;
    let local_commit = head.peel_to_commit()?;

    let (ahead, behind) = repo.graph_ahead_behind(local_commit.id(), upstream_commit.id())?;
    Ok((ahead > 0, behind > 0))
}

// *************************************************************************** //
// Repository Status Functions
// *************************************************************************** //

pub fn repo_work_status(repo: &Repository) -> Result<RepoWorkStatus, GitError> {
    let workdir_unsaved = match is_tree_clean(repo) {
        Ok(status) => Some(!status),
        Err(_) => None,
    };
    let index_unsaved = match is_index_clean(repo) {
        Ok(status) => Some(!status),
        Err(_) => None,
    };
    let (ahead_upstream, behind_upstream) = match upstream_status(repo) {
        Ok(status) => (Some(status.0), Some(status.1)),
        Err(_) => (None, None),
    };
    let diverged = match (ahead_upstream, behind_upstream) {
        (Some(true), Some(true)) => Some(true),
        _ => Some(false),
    };
    let submodule_exist = match repo.submodules() {
        Ok(submodules) => Some(submodules.len() > 0),
        Err(_) => None,
    };

    Ok(RepoWorkStatus {
        workdir_unsaved,
        index_unsaved,
        ahead_upstream,
        behind_upstream,
        submodule_exist,
        diverged,
    })
}

pub fn repo_check(repo: &Repository) -> Result<RepoCheck, GitError> {
    let is_bare = Some(repo.is_bare());

    let head_points_to_branch = match repo.head() {
        Ok(head) => Some(head.is_branch()),
        Err(_) => None,
    };

    let branch_exists = if head_points_to_branch.unwrap_or(false) {
        match repo.head() {
            Ok(head) => {
                let branch = head.shorthand().ok_or_else(|| git2::Error::from_str("Branch name error"))?;
                Some(repo.find_branch(branch, git2::BranchType::Local).is_ok())
            },
            Err(_) => None,
        }
    } else {
        None
    };

    let local_branch_count = match repo.branches(Some(git2::BranchType::Local)) {
        Ok(branches) => {
            let count = branches.count();
            Some(if count == 0 {
                BranchCount::Zero
            } else if count == 1 {
                BranchCount::One
            } else {
                BranchCount::Many
            })
        },
        Err(_) => None,
    };

    let remote_branch_count = match repo.branches(Some(git2::BranchType::Remote)) {
        Ok(branches) => {
            let count = branches.count();
            Some(if count == 0 {
                BranchCount::Zero
            } else if count == 1 {
                BranchCount::One
            } else {
                BranchCount::Many
            })
        },
        Err(_) => None,
    };

    let has_upstream = if branch_exists.unwrap_or(false) {
        match repo.head() {
            Ok(head) => {
                let branch = head.shorthand().ok_or_else(|| git2::Error::from_str("No branch found"))?;
                Some(repo.find_branch(branch, git2::BranchType::Local)?.upstream().is_ok())
            },
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(RepoCheck {
        is_bare,
        head_points_to_branch,
        branch_exists,
        local_branch_count,
        remote_branch_count,
        has_upstream,
    })
}

// *************************************************************************** //
// Default Configuration
// *************************************************************************** //

impl DionysiusConfig {
    pub fn git_default_config() -> &'static DionysiusConfig {
        static GIT_DEFAULT_CONFIG: OnceLock<DionysiusConfig> = OnceLock::new();
        GIT_DEFAULT_CONFIG.get_or_init(|| {
            let config = DionysiusConfig {
                // common: Some(CommonConfig {
                //     default_push: Some(vec!["origin".to_string()]),
                //     ignore: Some("dionysius".to_string()),
                //     ignore_list: Some(vec![]),
                //     posix_acl: Some(true),
                //     numeric_owner: Some(true),
                // }),
                trigger: None,
                git: Some(
                    PushTaskConfig::Git(
                        GitConfig::default()
                    )
                ),
                borg: None,
                // ntfs: None,
                // allow_modify: Some(false),
            };
            if !config.is_complete() {panic!()}
            config
        })
    }
}

pub fn git_repo_metadata(dir: &Path) -> Result<GitRepoMetaData, git2::Error> {
    let repo = Repository::open(dir)?;
    // let repo = Repository::open(dir);
    // if repo.is_err() {
    //     eprintln!("Failed to open repository: {}", dir.display());
    // }
    // let repo = repo.unwrap();
    let repo_check = repo_check(&repo).expect("Failed to check repository conditions");
    let work_status = repo_work_status(&repo).expect(format!("Failed to get status of repository: {}", dir.display()).as_str());
    Ok(
        GitRepoMetaData {
            worktree: dir.to_path_buf(),
            gitdir: repo.path().to_path_buf(),
            repo_check,
            work_status,
        }
    )
}
use std::convert::TryFrom;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Clone)]
pub enum BorgPattern {
    FnMatch(String),
    Shell(String),
    Regex(String),
    PathPrefix(String),
    PathFullMatch(String),
}

#[derive(Debug, PartialEq, Clone)]
pub struct GitIgnorePattern {
    pub pattern: String,
}

impl fmt::Display for BorgPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BorgPattern::FnMatch(p) => write!(f, "fm:{}", p),
            BorgPattern::Shell(p) => write!(f, "sh:{}", p),
            BorgPattern::Regex(p) => write!(f, "re:{}", p),
            BorgPattern::PathPrefix(p) => write!(f, "pp:{}", p),
            BorgPattern::PathFullMatch(p) => write!(f, "pf:{}", p),
        }
    }
}

impl TryFrom<String> for BorgPattern {
    type Error = &'static str;

    fn try_from(pattern_str: String) -> Result<Self, Self::Error> {
        if pattern_str.starts_with("fm:") {
            Ok(BorgPattern::FnMatch(pattern_str.chars().skip(3).collect()))
        } else if pattern_str.starts_with("sh:") {
            Ok(BorgPattern::Shell(pattern_str.chars().skip(3).collect()))
        } else if pattern_str.starts_with("re:") {
            Ok(BorgPattern::Regex(pattern_str.chars().skip(3).collect()))
        } else if pattern_str.starts_with("pp:") {
            Ok(BorgPattern::PathPrefix(pattern_str.chars().skip(3).collect()))
        } else if pattern_str.starts_with("pf:") {
            Ok(BorgPattern::PathFullMatch(pattern_str.chars().skip(3).collect()))
        } else {
            Err("Currently you must specify pattern type.")
        }
    }
}

impl TryFrom<String> for GitIgnorePattern {
    type Error = &'static str;

    fn try_from(pattern_str: String) -> Result<Self, Self::Error> {
        if pattern_str.starts_with("fm:") || pattern_str.starts_with("sh:") || pattern_str.starts_with("re:") || pattern_str.starts_with("pp:") || pattern_str.starts_with("pf:") {
            let borg_pattern = BorgPattern::try_from(pattern_str)?;
            return GitIgnorePattern::try_from(borg_pattern);
        }
        Ok(GitIgnorePattern { pattern: pattern_str })
    }
}

impl TryFrom<GitIgnorePattern> for BorgPattern {
    type Error = &'static str;

    fn try_from(git_pattern: GitIgnorePattern) -> Result<Self, Self::Error> {
        let mut pattern = git_pattern.pattern;

        if pattern.starts_with("!") {
            return Err("Negation is not supported");
        }
        if !pattern.replace("\\/", "").contains("/") {
            pattern = format!("**/{}", pattern);
        }
        if pattern.starts_with("/") {
            pattern = pattern.chars().skip(1).collect();
        }
        Ok(BorgPattern::Shell(pattern))
    }
}

impl TryFrom<BorgPattern> for GitIgnorePattern {
    type Error = &'static str;

    fn try_from(borg_pattern: BorgPattern) -> Result<Self, Self::Error> {
        match borg_pattern {
            BorgPattern::FnMatch(mut p) => Err("Cannot convert fnmatch pattern to gitignore pattern"),
            BorgPattern::Shell(mut p) => {
                if !p.starts_with('/') {
                    p = format!("/{}", p);
                }
                Ok(GitIgnorePattern { pattern: p })
            },
            BorgPattern::Regex(_) => Err("Cannot convert regex pattern to gitignore pattern"),
            BorgPattern::PathPrefix(mut p) => {
                if !p.starts_with('/') {
                    p = format!("/{}", p);
                }
                if !p.ends_with('/') {
                    p = format!("{}/", p);
                }
                Ok(GitIgnorePattern { pattern: p })
            },
            BorgPattern::PathFullMatch(mut p) => {
                if !p.starts_with('/') {
                    p = format!("/{}", p);
                }
                Ok(GitIgnorePattern { pattern: p })
            },
        }
    }
}

pub fn replace_possibly_escaped(str: String, replace_from: &str, replace_to: &str) -> String {
    let mut str_iter = str.chars().peekable();
    let mut from_iter = replace_from.chars();
    let mut result = String::new();
    // peek may be not ecnomical, should impl Iterator
    while let Some(&char) = str_iter.peek() {
        println!("{} - {}", char, result);
        if let Some(from_next) = from_iter.next() {
            str_iter.next(); // consume the character
            if char == '\\' {
                // '\\' will not occur in `replace_from`
                result.push('\\');
                result.push(str_iter.next().expect("Incomplete escaping.")); // skip the next character
                from_iter = replace_from.chars(); // reset from_iter
                // continue;
            } else {
                if char == from_next {
                    // match
                    // continue;
                } else {
                    // not match
                    from_iter = replace_from.chars(); // reset from_iter
                    result.push(char);
                }
            }
        }
        else {
            // match finished
            from_iter = replace_from.chars(); // reset from_iter
            result.push_str(replace_to);
        }
    }
    return result;
}

pub fn read_gitignore(file: &PathBuf) -> Vec<GitIgnorePattern> {
    let content = std::fs::read_to_string(file).unwrap();
    let mut patterns = Vec::new();
    for line in content.lines() {
        if line.starts_with("#") || line.trim().is_empty() {
            continue;
        }
        patterns.push(GitIgnorePattern { pattern: line.to_string() });
    }
    patterns
}

#[cfg(test)]
mod tests {
    use super::*;
    // use colored::*;

    #[test]
    fn test_replace_possibly_escaped() {
        let result = replace_possibly_escaped("a*\\*cc*dd".to_string(), "*", "??");
        assert_eq!(result, "a??\\*cc??dd");
    }

    #[test]
    fn test_gitignore_to_borgpattern() {
        let git_pattern = GitIgnorePattern { pattern: "test".to_string() };
        let borg_pattern: BorgPattern = git_pattern.try_into().unwrap();
        assert_eq!(borg_pattern, BorgPattern::Shell("**/test".to_string()));
    }

    #[test]
    fn test_borgpattern_to_gitignore() {
        let borg_pattern = BorgPattern::Shell("test".to_string());
        let git_pattern: GitIgnorePattern = borg_pattern.try_into().unwrap();
        assert_eq!(git_pattern.pattern, "/test");
    }

    #[test]
    fn test_read_gitignore() {
        let temp_file = std::env::temp_dir().join("test_gitignore");
        std::fs::write(&temp_file, "test\n# comment\n\n!negate").unwrap();
        let patterns = read_gitignore(&temp_file);
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0].pattern, "test");
        assert_eq!(patterns[1].pattern, "!negate");
    }

    #[test]
    fn test_gitignore_with_negation() {
        let git_pattern = GitIgnorePattern { pattern: "!test".to_string() };
        let result = BorgPattern::try_from(git_pattern);
        assert!(result.is_err());
    }

    #[test]
    fn test_gitignore_with_trailing_slash() {
        let git_pattern = GitIgnorePattern { pattern: "dir/".to_string() };
        let borg_pattern: BorgPattern = git_pattern.try_into().unwrap();
        assert_eq!(borg_pattern, BorgPattern::Shell("dir/".to_string()));
    }

    #[test]
    fn test_borgpattern_to_gitignore_with_trailing_slash() {
        let borg_pattern = BorgPattern::PathPrefix("dir/".to_string());
        let git_pattern: GitIgnorePattern = borg_pattern.try_into().unwrap();
        assert_eq!(git_pattern.pattern, "/dir/");
    }

    #[test]
    fn test_gitignore_with_leading_slash() {
        let git_pattern = GitIgnorePattern { pattern: "/dir/file".to_string() };
        let borg_pattern: BorgPattern = git_pattern.try_into().unwrap();
        assert_eq!(borg_pattern, BorgPattern::Shell("dir/file".to_string()));
    }

    #[test]
    fn test_borgpattern_to_gitignore_with_leading_slash() {
        let borg_pattern = BorgPattern::PathFullMatch("dir/file".to_string());
        let git_pattern: GitIgnorePattern = borg_pattern.try_into().unwrap();
        assert_eq!(git_pattern.pattern, "/dir/file");
    }

    #[test]
    fn test_cargo_gitignore() {
        let cargo_gitignore = r#"
# Generated by Cargo
# will have compiled files and executables
/debug
/target

# Remove Cargo.lock from gitignore if creating an executable, leave it for libraries
# More information here http://doc.crates.io/guide.html#cargotoml-vs-cargolock
Cargo.lock

# These are backup files generated by rustfmt
**/*.rs.bk
"#;
        let temp_file = std::env::temp_dir().join("cargo_gitignore");
        std::fs::write(&temp_file, cargo_gitignore).unwrap();
        let patterns = read_gitignore(&temp_file);
        assert_eq!(patterns.len(), 4);
        assert_eq!(patterns[0].pattern, "/debug");
        assert_eq!(patterns[1].pattern, "/target");
        assert_eq!(patterns[2].pattern, "Cargo.lock");
        assert_eq!(patterns[3].pattern, "**/*.rs.bk");

        let borg_patterns: Vec<BorgPattern> = patterns.into_iter().map(|p| p.try_into().unwrap()).collect();
        assert_eq!(borg_patterns.len(), 4);
        assert_eq!(borg_patterns[0], BorgPattern::Shell("debug".to_string()));
        assert_eq!(borg_patterns[1], BorgPattern::Shell("target".to_string()));
        assert_eq!(borg_patterns[2], BorgPattern::Shell("**/Cargo.lock".to_string()));
        assert_eq!(borg_patterns[3], BorgPattern::Shell("**/*.rs.bk".to_string()));
    }

    #[test]
    fn test_cmake_gitignore() {
        let cmake_gitignore = r#"
# Prerequisites
*.d

# Compiled Object files
*.slo
*.lo
*.o
*.obj

# Precompiled Headers
*.gch
*.pch

# Compiled Dynamic libraries
*.so
*.dylib
*.dll

# Fortran module files
*.mod
*.smod

# Compiled Static libraries
*.lai
*.la
*.a
*.lib

# Executables
*.exe
*.out
*.app
"#;
        let temp_file = std::env::temp_dir().join("cmake_gitignore");
        std::fs::write(&temp_file, cmake_gitignore).unwrap();
        let patterns = read_gitignore(&temp_file);
        assert_eq!(patterns.len(), 19);
        assert_eq!(patterns[0].pattern, "*.d");
        assert_eq!(patterns[1].pattern, "*.slo");
        assert_eq!(patterns[2].pattern, "*.lo");
        assert_eq!(patterns[3].pattern, "*.o");
        assert_eq!(patterns[4].pattern, "*.obj");
        assert_eq!(patterns[5].pattern, "*.gch");
        assert_eq!(patterns[6].pattern, "*.pch");
        assert_eq!(patterns[7].pattern, "*.so");
        assert_eq!(patterns[8].pattern, "*.dylib");
        assert_eq!(patterns[9].pattern, "*.dll");
        assert_eq!(patterns[10].pattern, "*.mod");
        assert_eq!(patterns[11].pattern, "*.smod");
        assert_eq!(patterns[12].pattern, "*.lai");
        assert_eq!(patterns[13].pattern, "*.la");
        assert_eq!(patterns[14].pattern, "*.a");
        assert_eq!(patterns[15].pattern, "*.lib");
        assert_eq!(patterns[16].pattern, "*.exe");
        assert_eq!(patterns[17].pattern, "*.out");
        assert_eq!(patterns[18].pattern, "*.app");

        let borg_patterns: Vec<BorgPattern> = patterns.into_iter().map(|p| p.try_into().unwrap()).collect();
        assert_eq!(borg_patterns.len(), 19);
        assert_eq!(borg_patterns[0], BorgPattern::Shell("**/*.d".to_string()));
        assert_eq!(borg_patterns[1], BorgPattern::Shell("**/*.slo".to_string()));
        assert_eq!(borg_patterns[2], BorgPattern::Shell("**/*.lo".to_string()));
        assert_eq!(borg_patterns[3], BorgPattern::Shell("**/*.o".to_string()));
        assert_eq!(borg_patterns[4], BorgPattern::Shell("**/*.obj".to_string()));
        assert_eq!(borg_patterns[5], BorgPattern::Shell("**/*.gch".to_string()));
        assert_eq!(borg_patterns[6], BorgPattern::Shell("**/*.pch".to_string()));
        assert_eq!(borg_patterns[7], BorgPattern::Shell("**/*.so".to_string()));
        assert_eq!(borg_patterns[8], BorgPattern::Shell("**/*.dylib".to_string()));
        assert_eq!(borg_patterns[9], BorgPattern::Shell("**/*.dll".to_string()));
        assert_eq!(borg_patterns[10], BorgPattern::Shell("**/*.mod".to_string()));
        assert_eq!(borg_patterns[11], BorgPattern::Shell("**/*.smod".to_string()));
        assert_eq!(borg_patterns[12], BorgPattern::Shell("**/*.lai".to_string()));
        assert_eq!(borg_patterns[13], BorgPattern::Shell("**/*.la".to_string()));
        assert_eq!(borg_patterns[14], BorgPattern::Shell("**/*.a".to_string()));
        assert_eq!(borg_patterns[15], BorgPattern::Shell("**/*.lib".to_string()));
        assert_eq!(borg_patterns[16], BorgPattern::Shell("**/*.exe".to_string()));
        assert_eq!(borg_patterns[17], BorgPattern::Shell("**/*.out".to_string()));
        assert_eq!(borg_patterns[18], BorgPattern::Shell("**/*.app".to_string()));
    }

    #[test]
    fn test_yarn_gitignore() {
        let yarn_gitignore = r#"
# Yarn Integrity file
.yarn-integrity

# Yarn Modules
.yarn/*
!.yarn/cache
!.yarn/patches
!.yarn/releases
!.yarn/plugins
!.yarn/sdks
!.yarn/versions

# Yarn Unplugged
.pnp.*
"#;
        let temp_file = std::env::temp_dir().join("yarn_gitignore");
        std::fs::write(&temp_file, yarn_gitignore).unwrap();
        let patterns = read_gitignore(&temp_file);
        assert_eq!(patterns.len(), 9);
        assert_eq!(patterns[0].pattern, ".yarn-integrity");
        assert_eq!(patterns[1].pattern, ".yarn/*");
        assert_eq!(patterns[2].pattern, "!.yarn/cache");
        assert_eq!(patterns[3].pattern, "!.yarn/patches");
        assert_eq!(patterns[4].pattern, "!.yarn/releases");
        assert_eq!(patterns[5].pattern, "!.yarn/plugins");
        assert_eq!(patterns[6].pattern, "!.yarn/sdks");
        assert_eq!(patterns[7].pattern, "!.yarn/versions");
        assert_eq!(patterns[8].pattern, ".pnp.*");
    }
}
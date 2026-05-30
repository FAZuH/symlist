use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::PathBuf;

use clap::CommandFactory;
use clap::Parser;
use clap::Subcommand;
use color_eyre::Result;
use color_eyre::eyre::Context;
use color_eyre::eyre::bail;
use color_eyre::eyre::eyre;

fn main() -> Result<()> {
    color_eyre::install()?;
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    let link_conf = match cli.link_conf {
        Some(c) => c,
        None => default_link_conf()?,
    };

    if !link_conf.exists() {
        bail!("config file {link_conf:?} does not exist")
    }

    let parser = SymlinkParser::new(File::open(link_conf)?)?;

    let symlinks: Vec<_> = parser.into_iter().collect();
    for link in symlinks {
        let SymlinkConf { original, link } = link?;

        let res = match cli.command {
            Command::Symlink => symlink(original, link),
            Command::Unlink => unlink(original, link),
        };

        if let Err(e) = res {
            println!("Error: {e:?}");
        }
    }

    Ok(())
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to symlink configurations. By default looks for `symlink.lst` in cwd.
    #[arg(short, long, global = true)]
    pub link_conf: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create symlinks of all entries at LINK_CONF
    Symlink,
    /// Unlink all entries at LINK_CONF
    Unlink,
}

fn symlink(original: PathBuf, link: PathBuf) -> Result<()> {
    if let Some(parent) = link.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).wrap_err(format!("failed creating dirs {parent:?}"))?
    }

    if link.symlink_metadata().is_ok() {
        if !link.is_symlink() {
            bail!("file {link:?} already exists but is not a symlink");
        }

        let link_dest = std::fs::read_link(&link)?;

        if link_dest != original {
            bail!(
                "symlink {link:?} already exists but links to {link_dest:?} instead of {original:?}"
            );
        }
    } else {
        std::os::unix::fs::symlink(&original, &link)
            .wrap_err(format!("failed creating symlink {link:?} to {original:?}"))?;
        println!("Successfully linked {link:?} to {original:?}");
    }

    Ok(())
}

fn unlink(original: PathBuf, link: PathBuf) -> Result<()> {
    if link.symlink_metadata().is_err() {
        bail!("symlink {link:?} does not exist")
    }

    if !link.is_symlink() {
        bail!("file {link:?} exists but is not a symlink")
    }

    std::fs::remove_file(&link).wrap_err("error removing symlink")?;

    println!("Successfully removed linked {link:?} to {original:?}");
    Ok(())
}

fn default_link_conf() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    Ok(cwd.join("symlink.lst"))
}

pub struct SymlinkConf {
    pub original: PathBuf,
    pub link: PathBuf,
}

pub struct SymlinkParser {
    reader: BufReader<File>,
    line_no: usize,
}

impl SymlinkParser {
    pub fn new(file: impl Into<File>) -> Result<Self> {
        Ok(Self {
            reader: BufReader::new(file.into()),
            line_no: 0,
        })
    }
}

impl Iterator for SymlinkParser {
    type Item = Result<SymlinkConf>;

    fn next(&mut self) -> Option<Self::Item> {
        self.line_no += 1;

        let mut buf = String::new();
        match self.reader.read_line(&mut buf) {
            Ok(0) => return None, // EOF
            Err(e) => return Some(Err(e.into())),
            _ => {}
        }

        let buf = buf.trim();

        // comment line
        if buf.starts_with("#") {
            return self.next();
        }

        // empty line
        if buf.is_empty() {
            return self.next();
        }

        let paths: Vec<_> = buf.split(":").collect();

        if paths.len() != 2 {
            return Some(Err(eyre!("invalid line at line {}: {}", self.line_no, buf)));
        }

        // guaranteed by the check above
        let original = PathBuf::from(paths.first().unwrap());
        let link = PathBuf::from(paths.get(1).unwrap());

        Some(Ok(SymlinkConf { original, link }))
    }
}

#[cfg(test)]
mod test {
    use std::io::Write;

    use super::*;

    struct TestEnv {
        dir: tempfile::TempDir,
        original: PathBuf,
        link: PathBuf,
    }

    impl TestEnv {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let original = dir.path().join("original.txt");
            File::create(&original).unwrap();
            let link = dir.path().join("link.txt");
            Self {
                dir,
                original,
                link,
            }
        }
    }

    #[test]
    fn symlink_parser() {
        let dir = tempfile::tempdir().unwrap();
        let conf = dir.path().join("symlink.lst");
        writeln!(File::create(&conf).unwrap(), "# foobar\n\nfoo:bar").unwrap();

        let mut parser = SymlinkParser::new(File::open(&conf).unwrap()).unwrap();
        let conf = parser.next().unwrap().unwrap();

        assert_eq!(conf.original, PathBuf::from("foo"));
        assert_eq!(conf.link, PathBuf::from("bar"));
    }

    #[test]
    fn symlink_basic() {
        let env = TestEnv::new();
        symlink(env.original.clone(), env.link.clone()).unwrap();

        assert!(env.link.is_symlink());
        assert_eq!(std::fs::read_link(&env.link).unwrap(), env.original);
    }

    #[test]
    fn symlink_creates_parent_dir() {
        let env = TestEnv::new();
        let link = env.dir.path().join("nested").join("link.txt");
        symlink(env.original.clone(), link.clone()).unwrap();

        assert!(link.is_symlink());
        assert_eq!(std::fs::read_link(&link).unwrap(), env.original);
    }

    #[test]
    fn symlink_already_exists_correct() {
        let env = TestEnv::new();
        std::os::unix::fs::symlink(&env.original, &env.link).unwrap();

        // Calling symlink again should succeed without error
        symlink(env.original.clone(), env.link.clone()).unwrap();

        assert!(env.link.is_symlink());
        assert_eq!(std::fs::read_link(&env.link).unwrap(), env.original);
    }

    #[test]
    fn symlink_already_exists_incorrect_target() {
        let env = TestEnv::new();
        let other_original = env.dir.path().join("other.txt");
        File::create(&other_original).unwrap();

        std::os::unix::fs::symlink(&other_original, &env.link).unwrap();

        // Calling symlink with a different target should fail
        let result = symlink(env.original.clone(), env.link.clone());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "symlink {:?} already exists but links to {:?} instead of {:?}",
                env.link, other_original, env.original
            )
        );
    }

    #[test]
    fn symlink_dangling_incorrect_target() {
        let env = TestEnv::new();
        let dangling_target = env.dir.path().join("nonexistent.txt");
        std::os::unix::fs::symlink(&dangling_target, &env.link).unwrap();

        let result = symlink(env.original.clone(), env.link.clone());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "symlink {:?} already exists but links to {:?} instead of {:?}",
                env.link, dangling_target, env.original
            )
        );
    }

    #[test]
    fn symlink_dangling_correct_target() {
        let env = TestEnv::new();
        let dangling_target = env.dir.path().join("nonexistent.txt");
        std::os::unix::fs::symlink(&dangling_target, &env.link).unwrap();

        symlink(dangling_target.clone(), env.link.clone()).unwrap();

        assert!(env.link.is_symlink());
        assert_eq!(std::fs::read_link(&env.link).unwrap(), dangling_target);
    }

    #[test]
    fn symlink_exists_not_symlink() {
        let env = TestEnv::new();
        File::create(&env.link).unwrap();

        let result = symlink(env.original.clone(), env.link.clone());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            format!("file {:?} already exists but is not a symlink", env.link)
        );
    }

    #[test]
    fn symlink_parser_invalid_format() {
        let dir = tempfile::tempdir().unwrap();
        let conf1 = dir.path().join("symlink1.lst");
        writeln!(File::create(&conf1).unwrap(), "invalid_line_no_colon").unwrap();
        let mut parser = SymlinkParser::new(File::open(&conf1).unwrap()).unwrap();
        assert!(parser.next().unwrap().is_err());

        let conf2 = dir.path().join("symlink2.lst");
        writeln!(File::create(&conf2).unwrap(), "foo:bar:baz").unwrap();
        let mut parser2 = SymlinkParser::new(File::open(&conf2).unwrap()).unwrap();
        assert!(parser2.next().unwrap().is_err());
    }

    #[test]
    fn symlink_parser_spaces_and_special() {
        let dir = tempfile::tempdir().unwrap();
        let conf = dir.path().join("symlink.lst");
        writeln!(File::create(&conf).unwrap(), "foo bar:baz/qux.txt").unwrap();

        let mut parser = SymlinkParser::new(File::open(&conf).unwrap()).unwrap();
        let conf = parser.next().unwrap().unwrap();
        assert_eq!(conf.original, PathBuf::from("foo bar"));
        assert_eq!(conf.link, PathBuf::from("baz/qux.txt"));
    }

    #[test]
    fn symlink_parser_only_comments_and_empty() {
        let dir = tempfile::tempdir().unwrap();
        let conf = dir.path().join("symlink.lst");
        writeln!(
            File::create(&conf).unwrap(),
            "# comment 1\n\n# comment 2\n   \n"
        )
        .unwrap();

        let mut parser = SymlinkParser::new(File::open(&conf).unwrap()).unwrap();
        assert!(parser.next().is_none());
    }

    #[test]
    fn symlink_parent_creation_fails() {
        let env = TestEnv::new();
        let parent = env.dir.path().join("parent_file");
        File::create(&parent).unwrap();

        let link = parent.join("link.txt");
        let result = symlink(env.original, link);
        assert!(result.is_err());
    }

    #[test]
    fn unlink_dangling_symlink() {
        let env = TestEnv::new();
        let dangling_target = env.dir.path().join("nonexistent.txt");

        std::os::unix::fs::symlink(&dangling_target, &env.link).unwrap();
        assert!(env.link.symlink_metadata().is_ok());
        assert!(!env.link.exists());

        unlink(env.original, env.link.clone()).unwrap();

        assert!(env.link.symlink_metadata().is_err());
    }

    #[test]
    fn unlink_removes_symlink() {
        let env = TestEnv::new();
        std::os::unix::fs::symlink(&env.original, &env.link).unwrap();
        assert!(env.link.is_symlink());

        unlink(env.original.clone(), env.link.clone()).unwrap();

        assert!(!env.link.exists());
        assert!(env.original.exists());
    }

    #[test]
    fn unlink_nonexistent_link() {
        let env = TestEnv::new();
        assert!(unlink(env.original, env.link).is_err());
    }

    #[test]
    fn unlink_not_a_symlink() {
        let env = TestEnv::new();
        File::create(&env.link).unwrap();

        let result = unlink(env.original.clone(), env.link.clone());
        assert!(result.is_err());
    }
}

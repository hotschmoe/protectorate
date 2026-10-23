//! Termux service registration and local diagnostics. Files are left in place on uninstall.
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

const SERVICE: &str = "protectorate-node";
const MARKER: &str = "# Managed by Protectorate Rust node install\n";

#[derive(Clone, Debug)]
pub struct Paths {
    pub prefix: PathBuf,
    pub user_home: PathBuf,
    pub home: PathBuf,
}

impl Paths {
    pub fn current(home: PathBuf) -> io::Result<Self> {
        let prefix = env::var_os("PREFIX")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/data/data/com.termux/files/usr"));
        let user_home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("HOME is unset"))?;
        let paths = Self {
            prefix,
            user_home,
            home,
        };
        paths.validate()?;
        Ok(paths)
    }
    pub fn validate(&self) -> io::Result<()> {
        for path in [&self.prefix, &self.user_home, &self.home] {
            if !path.is_absolute() || path.to_string_lossy().contains(['\n', '\r']) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Termux paths must be absolute and single-line",
                ));
            }
        }
        Ok(())
    }
    pub fn binary(&self) -> PathBuf {
        self.prefix.join("bin/protectorate-rs")
    }
    pub fn run(&self) -> PathBuf {
        self.prefix.join("var/service").join(SERVICE).join("run")
    }
    pub fn boot(&self) -> PathBuf {
        self.user_home.join(".termux/boot").join(SERVICE)
    }
}

pub fn validate_managed(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => {
            let text = fs::read(path)?;
            if text
                .splitn(2, |byte| *byte == b'\n')
                .nth(1)
                .is_some_and(|body| body.starts_with(MARKER.as_bytes()))
            {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("unmanaged service script: {}", path.display()),
                ))
            }
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("service path is not a regular file: {}", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn service_files(paths: &Paths, args: Vec<String>) -> io::Result<(String, String)> {
    paths.validate()?;
    let spec = protectorate_svcgen::Spec {
        role: protectorate_svcgen::ROLE_NODE.into(),
        binary: paths.binary().display().to_string(),
        home: paths.home.display().to_string(),
        args,
        ..Default::default()
    };
    let run = protectorate_svcgen::termux_run(&spec).map_err(io::Error::other)?;
    fn mark(script: &str) -> io::Result<String> {
        let (shebang, body) = script.split_once('\n').ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "service script has no shebang")
        })?;
        if !shebang.starts_with("#!") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "service script has no shebang",
            ));
        }
        Ok(format!("{shebang}\n{MARKER}{body}"))
    }
    Ok((mark(&run)?, mark(&protectorate_svcgen::termux_boot())?))
}

pub fn write_service_files(paths: &Paths, run: &str, boot: &str) -> io::Result<()> {
    for path in [paths.run(), paths.boot()] {
        validate_managed(&path)?;
    }
    for path in [paths.run(), paths.boot()] {
        fs::create_dir_all(path.parent().unwrap())?;
    }
    protectorate_fsutil::write_file(&paths.run(), run.as_bytes(), 0o755)?;
    protectorate_fsutil::write_file(&paths.boot(), boot.as_bytes(), 0o755)
}

pub fn command(binary: &Path, args: &[&str]) -> io::Result<()> {
    let output = Command::new(binary).args(args).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{} failed: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

pub fn doctor(paths: &Paths, version: &str) -> Vec<(&'static str, &'static str, String)> {
    let mut checks = vec![(
        "platform",
        "info",
        format!("android/{}, Protectorate {}", env::consts::ARCH, version),
    )];
    for (name, path) in [("startup run", paths.run()), ("startup boot", paths.boot())] {
        let result = match validate_managed(&path) {
            Ok(()) if path.is_file() => ("ok", path.display().to_string()),
            Ok(()) => ("FAIL", format!("missing {}", path.display())),
            Err(error) => ("FAIL", error.to_string()),
        };
        checks.push((name, result.0, result.1));
    }
    checks
}

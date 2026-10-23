//! Per-user Windows node installer. All external commands use argv, never a shell.

use std::{
    env,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

const TASK: &str = "Protectorate-Rust-Node";
const DESCRIPTION: &str = "Protectorate Rust node managed by protectorate-rs node install";

pub(super) struct Command {
    control: String,
    name: Option<String>,
    key: Option<String>,
    key_file: Option<PathBuf>,
    ca_cert: Option<PathBuf>,
    home: PathBuf,
    dry_run: bool,
    elevated: bool,
    serve_args: Vec<String>,
}

pub(super) struct Uninstall {
    home: PathBuf,
    dry_run: bool,
}

pub(super) fn parse_uninstall(mut args: impl Iterator<Item = String>) -> Result<Uninstall, String> {
    let mut home = None;
    let mut dry_run = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--home" if home.is_none() => {
                home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
            }
            "--dry-run" if !dry_run => dry_run = true,
            _ => {
                return Err(format!(
                    "unknown or duplicate node uninstall argument {flag:?}"
                ));
            }
        }
    }
    Ok(Uninstall {
        home: home.map_or_else(crate::default_home, Ok)?,
        dry_run,
    })
}

#[derive(Debug, PartialEq, Eq)]
enum UninstallAction {
    StopTask,
    DeleteTask,
}

fn uninstall_plan(managed_task_present: bool) -> Vec<UninstallAction> {
    if managed_task_present {
        vec![UninstallAction::StopTask, UninstallAction::DeleteTask]
    } else {
        Vec::new()
    }
}

pub(super) fn run_uninstall(command: Uninstall) -> Result<(), Box<dyn Error>> {
    // existing_task rejects an unrelated task with our name before any mutation.
    let task = existing_task()?;
    if let Some(xml) = &task {
        if !task_uses_home(xml, &command.home) {
            return Err(format!(
                "scheduled task {TASK} uses another node home; pass its --home to uninstall"
            )
            .into());
        }
    }
    let plan = uninstall_plan(task.is_some());
    if command.dry_run {
        println!("dry run: nothing will be changed");
        println!(
            "stop managed node process for {} if running",
            command.home.display()
        );
    } else if task.is_none() {
        // An older conhost task may have left its child behind after /End.
        stop_managed_process(&command.home)?;
    }
    if plan.is_empty() {
        println!("scheduled task {TASK} is already absent");
    }
    for action in plan {
        match action {
            UninstallAction::StopTask => {
                if command.dry_run {
                    println!("stop {TASK} if running");
                } else {
                    let _ = run_program("schtasks.exe", &["/End", "/TN", TASK]);
                    wait_task_stopped()?;
                    stop_managed_process(&command.home)?;
                    println!("stopped {TASK}");
                }
            }
            UninstallAction::DeleteTask => {
                if command.dry_run {
                    println!("unregister {TASK}");
                } else {
                    // Check ownership again after waiting; another process may
                    // have changed the task while it was stopping.
                    if existing_task()?.is_some() {
                        run_program("schtasks.exe", &["/Delete", "/TN", TASK, "/F"])?;
                        println!("unregistered {TASK}");
                    }
                }
            }
        }
    }
    println!("state retained: {}", command.home.display());
    if let Ok(binary) = installed_binary() {
        println!("binary retained: {}", binary.display());
    }
    Ok(())
}

pub(super) fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut control = None;
    let mut name = None;
    let mut key = None;
    let mut key_file = None;
    let mut ca_cert = None;
    let mut home = None;
    let mut dry_run = false;
    let mut elevated = false;
    let mut serve_args = Vec::new();
    while let Some(flag) = args.next() {
        if flag == "--" {
            serve_args.extend(args);
            break;
        }
        match flag.as_str() {
            "--control" if control.is_none() => {
                control = Some(args.next().ok_or("--control requires URL")?)
            }
            "--name" if name.is_none() => name = Some(args.next().ok_or("--name requires NAME")?),
            "--key" if key.is_none() => key = Some(args.next().ok_or("--key requires KEY")?),
            "--key-file" if key_file.is_none() => {
                key_file = Some(PathBuf::from(
                    args.next().ok_or("--key-file requires FILE")?,
                ))
            }
            "--ca-cert" if ca_cert.is_none() => {
                ca_cert = Some(PathBuf::from(args.next().ok_or("--ca-cert requires FILE")?))
            }
            "--home" if home.is_none() => {
                home = Some(PathBuf::from(args.next().ok_or("--home requires DIR")?))
            }
            "--dry-run" if !dry_run => dry_run = true,
            "--elevated" if !elevated => elevated = true,
            _ => {
                return Err(format!(
                    "unknown or duplicate node install argument {flag:?}"
                ));
            }
        }
    }
    let control = control.ok_or("node install requires --control URL")?;
    if !control.starts_with("https://") && !control.starts_with("http://") {
        return Err("--control must be an HTTP or HTTPS URL".into());
    }
    if key.is_some() && key_file.is_some() {
        return Err("--key and --key-file are exclusive".into());
    }
    if let Some(name) = &name {
        if name.is_empty()
            || name.len() > 63
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || name.starts_with('-')
            || name.ends_with('-')
        {
            return Err("--name must be a lowercase DNS label of at most 63 characters".into());
        }
    }
    if serve_args
        .iter()
        .any(|arg| arg == "--home" || arg == "--agent-token-file" || arg == "--")
    {
        return Err("serve flags cannot override --home or --agent-token-file".into());
    }
    let home = home.map_or_else(crate::default_home, Ok)?;
    let mut check = vec![
        "--home".to_owned(),
        home.display().to_string(),
        "--agent-token-file".to_owned(),
        home.join("agent-token").display().to_string(),
    ];
    check.extend(serve_args.iter().cloned());
    crate::parse_serve(check.into_iter())?;
    Ok(Command {
        control,
        name,
        key,
        key_file,
        ca_cert,
        home,
        dry_run,
        elevated,
        serve_args,
    })
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// CommandLineToArgvW compatible escaping for arguments passed to conhost.exe.
fn win_quote(value: &str) -> String {
    if !value.is_empty() && !value.contains([' ', '\t', '"']) {
        return value.to_owned();
    }
    let mut result = String::from("\"");
    let mut slashes = 0;
    for c in value.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                result.push_str(&"\\".repeat(slashes * 2 + 1));
                result.push('"');
                slashes = 0;
            }
            _ => {
                result.push_str(&"\\".repeat(slashes));
                slashes = 0;
                result.push(c);
            }
        }
    }
    result.push_str(&"\\".repeat(slashes * 2));
    result.push('"');
    result
}

fn task_xml(
    binary: &Path,
    home: &Path,
    token: &Path,
    serve_args: &[String],
    sid: &str,
    elevated: bool,
) -> String {
    let mut words = vec![
        "node".into(),
        "serve".into(),
        "--home".into(),
        home.display().to_string(),
        "--agent-token-file".into(),
        token.display().to_string(),
    ];
    words.extend(serve_args.iter().cloned());
    let command = format!(
        "--headless {} {}",
        win_quote(&binary.display().to_string()),
        words
            .iter()
            .map(|arg| win_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let level = if elevated {
        "HighestAvailable"
    } else {
        "LeastPrivilege"
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n  <RegistrationInfo><Description>{DESCRIPTION}</Description></RegistrationInfo>\n  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{sid}</UserId></LogonTrigger></Triggers>\n  <Principals><Principal id=\"Author\"><UserId>{sid}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>{level}</RunLevel></Principal></Principals>\n  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><Hidden>true</Hidden><RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure></Settings>\n  <Actions Context=\"Author\"><Exec><Command>%SystemRoot%\\System32\\conhost.exe</Command><Arguments>{}</Arguments></Exec></Actions>\n</Task>\n",
        xml_escape(&command)
    )
}

fn task_uses_home(xml: &str, home: &Path) -> bool {
    let quoted_home = xml_escape(&win_quote(&home.display().to_string()));
    let quoted_token = xml_escape(&win_quote(&home.join("agent-token").display().to_string()));
    xml.contains(&format!(
        "--home {quoted_home} --agent-token-file {quoted_token}"
    ))
}

fn task_bytes(xml: &str) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xfe];
    for unit in xml.encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

fn run_program(program: &str, args: &[&str]) -> io::Result<()> {
    let output = ProcessCommand::new(program).args(args).output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "{program} failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn wait_task_stopped() -> io::Result<()> {
    // Task Scheduler can acknowledge /End while conhost still holds the EXE open.
    // Wait for its process state before attempting to replace the binary.
    const WAIT: &str = "$ErrorActionPreference='Stop'; for($i=0;$i -lt 40;$i++){ if((Get-ScheduledTask -TaskName 'Protectorate-Rust-Node' -ErrorAction Stop).State -ne 'Running'){exit 0}; Start-Sleep -Milliseconds 250 }; throw 'node task did not stop within 10 seconds'";
    run_program(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-Command", WAIT],
    )
}

fn stop_managed_process(home: &Path) -> Result<(), Box<dyn Error>> {
    // Task Scheduler owns conhost.exe, not the child it starts with --headless.
    // Match both the installed executable and this home's serve arguments so
    // uninstall cannot stop another task or a manually launched node.
    const STOP: &str = r#"$ErrorActionPreference='Stop'; $binary=$env:PROTECTORATE_INSTALL_BINARY_PATH; $guardian=$env:PROTECTORATE_INSTALL_GUARDIAN_PATH; $nodeHome=$env:PROTECTORATE_INSTALL_HOME_PATH; $token=Join-Path $nodeHome 'agent-token'; function ArgPattern($name,$value) { $v=[regex]::Escape($value); return '(?:^|\s)--'+$name+'\s+(?:"'+$v+'"|'+$v+')(?=\s|$)' }; $hp=ArgPattern 'home' $nodeHome; $tp=ArgPattern 'agent-token-file' $token; function Matches { @(Get-CimInstance Win32_Process -Filter "Name='protectorate-rs.exe' OR Name='protectorate-rs-guardian.exe'" | Where-Object { $_.ExecutablePath -in @($guardian,$binary,($binary+'.old')) -and $_.CommandLine -match $hp -and $_.CommandLine -match $tp }) }; foreach($p in ((Matches) | Sort-Object { if($_.ExecutablePath -eq $guardian){0}else{1} })) { Stop-Process -Id $p.ProcessId -Force -ErrorAction Stop }; for($i=0;$i -lt 40;$i++) { if((Matches).Count -eq 0) { exit 0 }; Start-Sleep -Milliseconds 250 }; throw 'managed node process did not stop within 10 seconds'"#;
    let output = ProcessCommand::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", STOP])
        .env("PROTECTORATE_INSTALL_BINARY_PATH", installed_binary()?)
        .env("PROTECTORATE_INSTALL_GUARDIAN_PATH", installed_guardian()?)
        .env("PROTECTORATE_INSTALL_HOME_PATH", home)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "stop managed node process: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
        .into())
    }
}

fn current_sid() -> io::Result<String> {
    let output = ProcessCommand::new("whoami.exe")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("whoami /user failed"));
    }
    let value = String::from_utf8_lossy(&output.stdout);
    let start = value
        .find("S-1-")
        .ok_or_else(|| io::Error::other("whoami returned no user SID"))?;
    let sid: String = value[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-' || *c == 'S')
        .collect();
    if sid.len() < 8 {
        return Err(io::Error::other("whoami returned an invalid user SID"));
    }
    Ok(sid)
}

fn restrict_directory(path: &Path) -> io::Result<()> {
    // The script receives the path in an environment variable so quote or shell
    // characters in a user profile path are never evaluated as PowerShell.
    const ACL: &str = "$ErrorActionPreference='Stop'; $p=$env:PROTECTORATE_INSTALL_ACL_PATH; $u=[Security.Principal.WindowsIdentity]::GetCurrent().User; $s=[Security.Principal.SecurityIdentifier]::new('S-1-5-18'); $a=[Security.AccessControl.DirectorySecurity]::new(); $a.SetOwner($u); $a.SetAccessRuleProtection($true,$false); $i=[Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit; foreach($id in @($u,$s)){$r=[Security.AccessControl.FileSystemAccessRule]::new($id,[Security.AccessControl.FileSystemRights]::FullControl,$i,[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow);$a.AddAccessRule($r)}; Set-Acl -LiteralPath $p -AclObject $a";
    let output = ProcessCommand::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", ACL])
        .env("PROTECTORATE_INSTALL_ACL_PATH", path)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "restrict directory {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn restrict_token(path: &Path) -> io::Result<()> {
    const ACL: &str = "$ErrorActionPreference='Stop'; $p=$env:PROTECTORATE_INSTALL_ACL_PATH; $u=[Security.Principal.WindowsIdentity]::GetCurrent().User; $s=[Security.Principal.SecurityIdentifier]::new('S-1-5-18'); $a=[Security.AccessControl.FileSecurity]::new(); $a.SetOwner($u); $a.SetAccessRuleProtection($true,$false); foreach($id in @($u,$s)){$r=[Security.AccessControl.FileSystemAccessRule]::new($id,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow);$a.AddAccessRule($r)}; Set-Acl -LiteralPath $p -AclObject $a";
    let output = ProcessCommand::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", ACL])
        .env("PROTECTORATE_INSTALL_ACL_PATH", path)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "restrict token {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn safe_dir(path: &Path) -> io::Result<bool> {
    use std::os::windows::fs::MetadataExt;
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && meta.file_attributes() & 0x400 == 0 => {
            restrict_directory(path)?;
            Ok(false)
        }
        Ok(_) => Err(io::Error::other(format!(
            "{} is not an ordinary directory",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            restrict_directory(path)?;
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

fn task_output_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xff, 0xfe])
        || (bytes.len() >= 8 && bytes.len().is_multiple_of(2) && bytes[1] == 0 && bytes[3] == 0)
    {
        let bytes = if bytes.starts_with(&[0xff, 0xfe]) {
            &bytes[2..]
        } else {
            bytes
        };
        return String::from_utf16_lossy(
            &bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        );
    }
    if bytes.starts_with(&[0xfe, 0xff])
        || (bytes.len() >= 8 && bytes.len().is_multiple_of(2) && bytes[0] == 0 && bytes[2] == 0)
    {
        let bytes = if bytes.starts_with(&[0xfe, 0xff]) {
            &bytes[2..]
        } else {
            bytes
        };
        return String::from_utf16_lossy(
            &bytes
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        );
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn existing_task() -> io::Result<Option<String>> {
    let output = ProcessCommand::new("schtasks.exe")
        .args(["/Query", "/TN", TASK, "/XML"])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let xml = task_output_text(&output.stdout);
    if xml.contains(DESCRIPTION)
        || xml.contains("Protectorate Rust node managed by install-node-windows.ps1")
    {
        Ok(Some(xml))
    } else {
        Err(io::Error::other(format!(
            "scheduled task {TASK} already exists and is not managed by Protectorate"
        )))
    }
}

fn installed_binary() -> Result<PathBuf, Box<dyn Error>> {
    let local = env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unset")?;
    Ok(PathBuf::from(local)
        .join("Programs")
        .join("Protectorate-Rust-Node")
        .join("protectorate-rs.exe"))
}

fn installed_guardian() -> Result<PathBuf, Box<dyn Error>> {
    Ok(installed_binary()?.with_file_name("protectorate-rs-guardian.exe"))
}

pub(super) async fn run(command: Command) -> Result<(), Box<dyn Error>> {
    let source = env::current_exe()?;
    let binary = installed_binary()?;
    let guardian = installed_guardian()?;
    let install_dir = binary.parent().ok_or("invalid install path")?;
    let token = command.home.join("agent-token");
    let xml_path = command.home.join("Protectorate-Rust-Node.xml");
    let joined = command.home.join("tsnet").join("join.json").try_exists()?;
    if command.dry_run {
        println!("dry run: nothing will be changed");
        println!("1. stop {TASK} if running");
        println!(
            "2. copy {} -> {} and {}",
            source.display(),
            binary.display(),
            guardian.display()
        );
        println!(
            "3. create private node state at {} and agent token if needed",
            command.home.display()
        );
        println!(
            "4. {}",
            if joined {
                "keep existing enrollment"
            } else {
                "enroll node with the control server"
            }
        );
        println!(
            "5. register and start current-user logon task {TASK} from {}",
            xml_path.display()
        );
        return Ok(());
    }
    let sid = current_sid()?;
    let previous_task = existing_task()?;
    if let Some(xml) = &previous_task {
        if !task_uses_home(xml, &command.home) {
            return Err(format!(
                "scheduled task {TASK} uses another node home; uninstall it with that --home first"
            )
            .into());
        }
    }
    let new_home = safe_dir(&command.home)?;
    let new_install = match safe_dir(install_dir) {
        Ok(created) => created,
        Err(error) => {
            if new_home {
                let _ = fs::remove_dir(&command.home);
            }
            return Err(error.into());
        }
    };
    if previous_task.is_some() {
        let _ = run_program("schtasks.exe", &["/End", "/TN", TASK]);
    }
    let stopped = (|| -> Result<(), Box<dyn Error>> {
        if previous_task.is_some() {
            wait_task_stopped()?;
        }
        // Also recover a child orphaned by an older conhost task that has
        // already been unregistered.
        stop_managed_process(&command.home)?;
        Ok(())
    })();
    if let Err(error) = stopped {
        if new_install {
            let _ = fs::remove_dir(install_dir);
        }
        if new_home {
            let _ = fs::remove_dir(&command.home);
        }
        return Err(error);
    }
    let old_binary = binary.exists();
    let old_guardian = guardian.exists();
    let backup = install_dir.join("protectorate-rs.previous.exe");
    let guardian_backup = install_dir.join("protectorate-rs-guardian.previous.exe");
    let source_is_target = fs::canonicalize(&source).ok() == fs::canonicalize(&binary).ok();
    let source_is_guardian = fs::canonicalize(&source).ok() == fs::canonicalize(&guardian).ok();
    let mut registered = false;
    let result = async {
        if !source_is_target {
            if old_binary {
                fs::copy(&binary, &backup)?;
            }
            protectorate_fsutil::retry(|| fs::copy(&source, &binary))?;
        }
        if !source_is_guardian {
            if old_guardian {
                fs::copy(&guardian, &guardian_backup)?;
            }
            protectorate_fsutil::retry(|| fs::copy(&source, &guardian))?;
        }
        if !token.exists() {
            let mut random = [0u8; 32];
            getrandom::getrandom(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
            let secret = random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            protectorate_fsutil::write_file(&token, secret.as_bytes(), 0o600)?;
            random.fill(0);
        } else {
            crate::read_token(&token)?;
        }
        restrict_token(&token)?;
        if !joined {
            let mut join = vec![
                "--control".into(),
                command.control.clone(),
                "--home".into(),
                command.home.display().to_string(),
            ];
            if let Some(name) = &command.name {
                join.extend(["--name".into(), name.clone()]);
            }
            if let Some(key) = &command.key {
                join.extend(["--key".into(), key.clone()]);
            }
            if let Some(key_file) = &command.key_file {
                join.extend(["--key-file".into(), key_file.display().to_string()]);
            }
            if let Some(ca) = &command.ca_cert {
                join.extend(["--ca-cert".into(), ca.display().to_string()]);
            }
            super::node_join::run(
                super::node_join::parse(join.into_iter()).map_err(io::Error::other)?,
            )
            .await?;
        }
        let xml = task_xml(
            &guardian,
            &command.home,
            &token,
            &command.serve_args,
            &sid,
            command.elevated,
        );
        protectorate_fsutil::write_file(&xml_path, &task_bytes(&xml), 0o600)?;
        run_program(
            "schtasks.exe",
            &[
                "/Create",
                "/TN",
                TASK,
                "/XML",
                xml_path.to_str().ok_or("non-Unicode task path")?,
                "/F",
            ],
        )?;
        registered = true;
        run_program("schtasks.exe", &["/Run", "/TN", TASK])?;
        Ok::<_, Box<dyn Error>>(())
    }
    .await;
    if let Err(error) = result {
        if registered {
            let _ = run_program("schtasks.exe", &["/End", "/TN", TASK]);
            let _ = wait_task_stopped();
            let _ = stop_managed_process(&command.home);
            let _ = run_program("schtasks.exe", &["/Delete", "/TN", TASK, "/F"]);
        }
        if !source_is_target {
            if old_binary && backup.exists() {
                let _ = fs::copy(&backup, &binary);
            } else if !old_binary {
                let _ = fs::remove_file(&binary);
            }
        }
        if !source_is_guardian {
            if old_guardian && guardian_backup.exists() {
                let _ = fs::copy(&guardian_backup, &guardian);
            } else if !old_guardian {
                let _ = fs::remove_file(&guardian);
            }
        }
        if let Some(previous) = previous_task {
            let restore_path = command.home.join("Protectorate-Rust-Node.restore.xml");
            if protectorate_fsutil::write_file(&restore_path, &task_bytes(&previous), 0o600).is_ok()
            {
                if let Some(path) = restore_path.to_str() {
                    let _ = run_program(
                        "schtasks.exe",
                        &["/Create", "/TN", TASK, "/XML", path, "/F"],
                    );
                }
                let _ = fs::remove_file(&restore_path);
            }
            let _ = run_program("schtasks.exe", &["/Run", "/TN", TASK]);
        }
        if new_install {
            let _ = fs::remove_dir_all(install_dir);
        }
        if new_home {
            let _ = fs::remove_dir_all(&command.home);
        }
        return Err(error);
    }
    let _ = fs::remove_file(backup);
    let _ = fs::remove_file(guardian_backup);
    println!(
        "installed node at {}; task {TASK} started",
        binary.display()
    );
    Ok(())
}

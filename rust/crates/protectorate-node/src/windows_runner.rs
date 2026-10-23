//! Dedicated Windows account for shared sandboxed jobs. Provisioning must run
//! elevated as the same user that later runs the node agent.
use crate::windows_process::{resume_process, ProcessTree};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::c_void,
    fs, io,
    os::windows::{ffi::OsStrExt, io::FromRawHandle},
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, DuplicateHandle, LocalFree, SetHandleInformation, DUPLICATE_SAME_ACCESS,
        GENERIC_READ, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    },
    NetworkManagement::NetManagement::{
        NERR_UserExists, NetUserAdd, NetUserDel, UF_DONT_EXPIRE_PASSWD, UF_PASSWD_CANT_CHANGE,
        UF_SCRIPT, USER_INFO_1, USER_PRIV_USER,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SetNamedSecurityInfoW, SE_FILE_OBJECT,
        },
        Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
        GetSecurityDescriptorDacl, GetTokenInformation, LookupAccountNameW, TokenUser,
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SID_NAME_USE, TOKEN_QUERY,
        TOKEN_USER,
    },
    Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
    System::{
        Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock},
        Pipes::CreatePipe,
        Threading::{
            GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, TerminateProcess,
            WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
            INFINITE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
        },
    },
};
use zeroize::Zeroizing;

const RUNNER_USER: &str = "protectorate-runner";
const CONFIG_FILE: &str = "runner-rust.json";
const ENTROPY: &[u8] = b"protectorate-rust-runner-v1";

#[link(name = "advapi32")]
unsafe extern "system" {
    fn LogonUserW(
        user: *const u16,
        domain: *const u16,
        password: *const u16,
        logon_type: u32,
        provider: u32,
        token: *mut HANDLE,
    ) -> i32;
    fn CreateProcessWithLogonW(
        user: *const u16,
        domain: *const u16,
        password: *const u16,
        logon_flags: u32,
        application_name: *const u16,
        command_line: *mut u16,
        creation_flags: u32,
        environment: *const c_void,
        current_directory: *const u16,
        startup_info: *const STARTUPINFOW,
        process_information: *mut PROCESS_INFORMATION,
    ) -> i32;
}

#[derive(Serialize, Deserialize)]
struct RunnerConfig {
    user: String,
    job_dir: PathBuf,
    password_dpapi: Vec<u8>,
}

pub(crate) struct Runner {
    pub(crate) user: String,
    pub(crate) root: PathBuf,
    password: Zeroizing<String>,
}

pub(crate) struct RunnerProcess {
    job: ProcessTree,
    process: usize,
    output: Option<fs::File>,
}

impl RunnerProcess {
    pub(crate) fn take_output(&mut self) -> Option<fs::File> {
        self.output.take()
    }
    pub(crate) fn terminate(&self) -> io::Result<()> {
        self.job.terminate()
    }
    pub(crate) fn wait_blocking(&self) -> io::Result<i32> {
        // SAFETY: this process handle remains owned by RunnerProcess throughout the wait.
        let status = unsafe { WaitForSingleObject(self.process as HANDLE, INFINITE) };
        if status != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut code = 0;
        // SAFETY: the process handle is valid after a successful wait.
        if unsafe { GetExitCodeProcess(self.process as HANDLE, &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(code as i32)
    }
    pub(crate) async fn wait(&self) -> io::Result<i32> {
        let process = {
            let mut duplicate = ptr::null_mut();
            // SAFETY: a duplicate keeps the handle live if this wait future is cancelled.
            if unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    self.process as HANDLE,
                    GetCurrentProcess(),
                    &mut duplicate,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            OwnedHandle::new(duplicate)
        };
        tokio::task::spawn_blocking(move || {
            // SAFETY: RunnerProcess owns the handle for the duration of this wait.
            let status = unsafe { WaitForSingleObject(process.handle(), INFINITE) };
            if status != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut code = 0;
            if unsafe { GetExitCodeProcess(process.handle(), &mut code) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(code as i32)
        })
        .await
        .map_err(io::Error::other)?
    }
}

impl Drop for RunnerProcess {
    fn drop(&mut self) {
        // SAFETY: CreateProcessWithLogonW returned this owned process handle.
        unsafe {
            CloseHandle(self.process as HANDLE);
        }
    }
}

impl Runner {
    pub(crate) fn spawn(
        &self,
        command: &[String],
        env: &BTreeMap<String, String>,
        workdir: &Path,
        job_id: &str,
        cpu_percent: u32,
        memory_bytes: u64,
    ) -> io::Result<RunnerProcess> {
        let workdir = fs::canonicalize(workdir)?;
        if !workdir.starts_with(&self.root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "runner workdir is outside shared root",
            ));
        }
        let process_workdir = process_path(&workdir)?;
        let token = OwnedHandle::new(logon(&self.user, &self.password)?);
        let environment = runner_environment(token.handle(), env, job_id, &process_workdir)?;
        let attrs = windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<windows_sys::Win32::Security::SECURITY_ATTRIBUTES>()
                as u32,
            lpSecurityDescriptor: ptr::null_mut(),
            bInheritHandle: 1,
        };
        let mut rd = ptr::null_mut();
        let mut wr = ptr::null_mut();
        // SAFETY: CreatePipe initializes both owned handles using valid attributes.
        if unsafe { CreatePipe(&mut rd, &mut wr, &attrs, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let rd = OwnedHandle::new(rd);
        let wr = OwnedHandle::new(wr);
        // SAFETY: only the child-side pipe must be inherited.
        if unsafe { SetHandleInformation(rd.handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let nul = wide("NUL");
        // SAFETY: NUL is a Windows device, opened as inheritable stdin.
        let stdin = unsafe {
            CreateFileW(
                nul.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                &attrs,
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if stdin == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let stdin = OwnedHandle::new(stdin);
        let job = ProcessTree::new(cpu_percent, memory_bytes)?;
        let startup = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            dwFlags: STARTF_USESTDHANDLES,
            hStdInput: stdin.handle(),
            hStdOutput: wr.handle(),
            hStdError: wr.handle(),
            ..Default::default()
        };
        let user = wide(&self.user);
        let domain = wide(".");
        let mut password = wide(&self.password);
        let mut line = wide(&windows_command_line(command));
        let directory = wide_path(&process_workdir);
        let mut process = PROCESS_INFORMATION::default();
        // SAFETY: all pointers are valid for this call and the child starts suspended.
        let started = unsafe {
            CreateProcessWithLogonW(
                user.as_ptr(),
                domain.as_ptr(),
                password.as_ptr(),
                1,
                ptr::null(),
                line.as_mut_ptr(),
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
                environment.as_ptr().cast(),
                directory.as_ptr(),
                &startup,
                &mut process,
            )
        };
        password.fill(0);
        if started == 0 {
            return Err(io::Error::last_os_error());
        }
        let process_handle = OwnedHandle::new(process.hProcess);
        let thread = OwnedHandle::new(process.hThread);
        if let Err(error) = job
            .attach_process(process_handle.handle())
            .and_then(|_| resume_process(process_handle.handle()))
        {
            // SAFETY: the suspended process is ours; it may not be a job member yet.
            unsafe {
                TerminateProcess(process_handle.handle(), 1);
            }
            return Err(error);
        }
        drop(thread);
        drop(wr);
        let output = rd.into_file();
        Ok(RunnerProcess {
            job,
            process: process_handle.into_usize(),
            output: Some(output),
        })
    }
}

struct OwnedHandle(usize);
impl OwnedHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle as usize)
    }
    fn handle(&self) -> HANDLE {
        self.0 as HANDLE
    }
    fn into_usize(self) -> usize {
        let value = self.0;
        std::mem::forget(self);
        value
    }
    fn into_file(self) -> fs::File {
        // SAFETY: CreatePipe gave an owned handle that File will close.
        unsafe { fs::File::from_raw_handle(self.into_usize() as *mut c_void) }
    }
}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle());
        }
    }
}

fn runner_environment(
    token: HANDLE,
    job: &BTreeMap<String, String>,
    id: &str,
    workdir: &Path,
) -> io::Result<Vec<u16>> {
    let mut block = ptr::null_mut();
    // SAFETY: the token is a valid LogonUserW token; userenv allocates the block.
    if unsafe { CreateEnvironmentBlock(&mut block, token, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let parsed = (|| {
        const MAX_ENV_UNITS: usize = 131_072;
        let mut values = BTreeMap::<String, (String, String)>::new();
        let mut offset = 0usize;
        loop {
            if offset >= MAX_ENV_UNITS {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "runner environment is too large",
                ));
            }
            let start = offset;
            // SAFETY: CreateEnvironmentBlock provides a double-NUL-terminated
            // UTF-16 block. The cap also bounds the scan if that contract fails.
            while offset < MAX_ENV_UNITS && unsafe { *(block as *const u16).add(offset) } != 0 {
                offset += 1;
            }
            if offset >= MAX_ENV_UNITS {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "runner environment is too large",
                ));
            }
            if offset == start {
                return Ok(values);
            }
            // SAFETY: the bounded scan found the row's terminating NUL.
            let row = String::from_utf16_lossy(unsafe {
                std::slice::from_raw_parts((block as *const u16).add(start), offset - start)
            });
            if let Some((key, value)) = row.split_once('=') {
                if !key.is_empty() {
                    values.insert(key.to_ascii_uppercase(), (key.into(), value.into()));
                }
            }
            offset += 1;
        }
    })();
    // SAFETY: CreateEnvironmentBlock allocated this pointer.
    unsafe {
        DestroyEnvironmentBlock(block);
    }
    let mut values = parsed?;
    for (key, value) in job {
        values.insert(key.to_ascii_uppercase(), (key.clone(), value.clone()));
    }
    values.insert(
        "PROTECTORATE_JOB_ID".into(),
        ("PROTECTORATE_JOB_ID".into(), id.into()),
    );
    values.insert(
        "PROTECTORATE_JOB_DIR".into(),
        (
            "PROTECTORATE_JOB_DIR".into(),
            workdir.to_string_lossy().into_owned(),
        ),
    );
    let mut output = Vec::new();
    for (_, (key, value)) in values {
        output.extend(format!("{key}={value}").encode_utf16());
        output.push(0);
    }
    if output.is_empty() {
        output.push(0);
    }
    output.push(0);
    Ok(output)
}

fn windows_command_line(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if !arg.is_empty()
                && !arg
                    .chars()
                    .any(|c| matches!(c, ' ' | '\t' | '\n' | '\u{b}' | '"'))
            {
                return arg.clone();
            }
            let mut quoted = String::from("\"");
            let mut slashes = 0;
            for ch in arg.chars() {
                match ch {
                    '\\' => {
                        slashes += 1;
                        continue;
                    }
                    '"' => {
                        quoted.push_str(&"\\".repeat(2 * slashes + 1));
                    }
                    _ => {
                        quoted.push_str(&"\\".repeat(slashes));
                    }
                }
                slashes = 0;
                quoted.push(ch);
            }
            quoted.push_str(&"\\".repeat(2 * slashes));
            quoted.push('"');
            quoted
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Runner {
    pub(crate) fn load(home: &Path) -> io::Result<Option<Self>> {
        let path = home.join(CONFIG_FILE);
        let raw = match fs::read(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if raw.len() > 16 << 10 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "runner config is too large",
            ));
        }
        let config: RunnerConfig = serde_json::from_slice(&raw)?;
        if config.user != RUNNER_USER
            || !config.job_dir.is_absolute()
            || config.password_dpapi.is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "runner config is incomplete",
            ));
        }
        let root = fs::canonicalize(config.job_dir)?;
        let home = fs::canonicalize(home)?;
        separate_roots(&home, &root)?;
        let unsealed = Zeroizing::new(unseal(&config.password_dpapi)?);
        let password = Zeroizing::new(String::from_utf8(unsealed.to_vec()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "runner password is not UTF-8")
        })?);
        let token = logon(&config.user, &password)?;
        // SAFETY: LogonUserW returned an owned token handle.
        unsafe {
            CloseHandle(token);
        }
        Ok(Some(Self {
            user: config.user,
            root,
            password,
        }))
    }
}

/// One-time account and shared-root setup. The caller must run elevated as
/// the agent user; this never changes the agent's own profile ACL.
pub fn install_windows_runner(home: &Path, job_root: &Path) -> io::Result<()> {
    if home.join(CONFIG_FILE).exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "runner is already configured",
        ));
    }
    if !job_root.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runner job root must be absolute",
        ));
    }
    if fs::symlink_metadata(job_root).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "runner job root must be a new directory",
        ));
    }
    fs::create_dir_all(home)?;
    let parent = job_root.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "runner job root has no parent")
    })?;
    fs::create_dir_all(parent)?;
    let home = fs::canonicalize(home)?;
    let root = fs::canonicalize(parent)?.join(job_root.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "runner job root has no name")
    })?);
    separate_roots(&home, &root)?;
    let mut random = [0u8; 28];
    getrandom::getrandom(&mut random).map_err(|error| io::Error::other(error.to_string()))?;
    let password = Zeroizing::new(runner_password(&random));
    ensure_account(RUNNER_USER, &password)
        .map_err(|error| io::Error::new(error.kind(), format!("create runner account: {error}")))?;
    let temp = home.join("runner-rust.json.new");
    let result = (|| {
        let token = logon(RUNNER_USER, &password).map_err(|error| {
            io::Error::new(error.kind(), format!("log on runner account: {error}"))
        })?;
        // SAFETY: LogonUserW returned an owned token handle.
        unsafe {
            CloseHandle(token);
        }
        fs::create_dir(&root).map_err(|error| {
            io::Error::new(error.kind(), format!("create runner root: {error}"))
        })?;
        protect_shared_root(&root, RUNNER_USER).map_err(|error| {
            io::Error::new(error.kind(), format!("protect runner root: {error}"))
        })?;
        let config = RunnerConfig {
            user: RUNNER_USER.into(),
            job_dir: root.clone(),
            password_dpapi: seal(password.as_bytes()).map_err(|error| {
                io::Error::new(error.kind(), format!("seal runner password: {error}"))
            })?,
        };
        fs::write(&temp, serde_json::to_vec(&config)?)?;
        fs::rename(&temp, home.join(CONFIG_FILE))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
        let _ = fs::remove_dir(&root); // only succeeds if no files were added
        let name = wide(RUNNER_USER);
        // SAFETY: this account was created by ensure_account above.
        unsafe {
            NetUserDel(ptr::null(), name.as_ptr());
        }
    }
    result
}

fn separate_roots(home: &Path, root: &Path) -> io::Result<()> {
    let components = |path: &Path| {
        path.components()
            .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>()
    };
    let home = components(home);
    let root = components(root);
    if home.starts_with(&root) || root.starts_with(&home) {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runner job root must be separate from agent state",
        ))
    } else {
        Ok(())
    }
}

fn runner_password(random: &[u8; 28]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut password = random
        .iter()
        .map(|byte| ALPHABET[*byte as usize % ALPHABET.len()] as char)
        .collect::<String>();
    password.push_str("Aa9#");
    password
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub(crate) fn process_path(path: &Path) -> io::Result<PathBuf> {
    let text = path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "runner path is not valid UTF-16",
        )
    })?;
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return Ok(PathBuf::from(format!(r"\\{rest}")));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        if rest.as_bytes().get(1) != Some(&b':') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "runner path is not a drive or UNC path",
            ));
        }
        return Ok(PathBuf::from(rest));
    }
    Ok(path.to_path_buf())
}

fn ensure_account(user: &str, password: &str) -> io::Result<()> {
    let mut user_w = wide(user);
    let mut password_w = wide(password);
    let mut comment = wide("Protectorate Rust shared job runner");
    let info = USER_INFO_1 {
        usri1_name: user_w.as_mut_ptr(),
        usri1_password: password_w.as_mut_ptr(),
        usri1_password_age: 0,
        usri1_priv: USER_PRIV_USER,
        usri1_home_dir: ptr::null_mut(),
        usri1_comment: comment.as_mut_ptr(),
        usri1_flags: UF_SCRIPT | UF_DONT_EXPIRE_PASSWD | UF_PASSWD_CANT_CHANGE,
        usri1_script_path: ptr::null_mut(),
    };
    // SAFETY: USER_INFO_1 pointers remain valid throughout NetUserAdd.
    let status = unsafe {
        NetUserAdd(
            ptr::null(),
            1,
            (&info as *const USER_INFO_1).cast(),
            ptr::null_mut(),
        )
    };
    // Never take over an existing account whose group membership we did not set.
    let status = if status == NERR_UserExists {
        183
    } else {
        status
    };
    password_w.fill(0);
    if status != 0 {
        Err(io::Error::from_raw_os_error(status as i32))
    } else {
        Ok(())
    }
}

fn logon(user: &str, password: &str) -> io::Result<HANDLE> {
    let user = wide(user);
    let domain = wide(".");
    let mut password = wide(password);
    let mut token = ptr::null_mut();
    // SAFETY: all strings are NUL terminated and token receives an owned handle.
    let result = unsafe {
        LogonUserW(
            user.as_ptr(),
            domain.as_ptr(),
            password.as_ptr(),
            2,
            0,
            &mut token,
        )
    };
    password.fill(0);
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(token)
    }
}

fn seal(secret: &[u8]) -> io::Result<Vec<u8>> {
    crypt(secret, false)
}
fn unseal(secret: &[u8]) -> io::Result<Vec<u8>> {
    crypt(secret, true)
}
fn crypt(secret: &[u8], decrypt: bool) -> io::Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: input and entropy buffers remain valid; crypt32 allocates output.
    let result = unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                ptr::null(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: crypt32 returned exactly cbData bytes and LocalFree owns the buffer.
    let value =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(value)
}

fn protect_shared_root(root: &Path, runner: &str) -> io::Result<()> {
    let agent_sid = current_user_sid()?;
    let runner_sid = account_sid(runner)?;
    let sddl = format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{agent_sid})(A;OICI;0x1301bf;;;{runner_sid})");
    set_protected_dacl(root, &sddl)
}

/// Publish a node-owned Git bundle for runner reads without granting runner
/// write access to its containing directory.
pub(crate) fn grant_runner_read(path: &Path) -> io::Result<()> {
    let agent_sid = current_user_sid()?;
    let runner_sid = account_sid(RUNNER_USER)?;
    let sddl = format!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;{agent_sid})(A;;GR;;;{runner_sid})");
    set_protected_dacl(path, &sddl)
}

fn set_protected_dacl(path: &Path, sddl: &str) -> io::Result<()> {
    let sddl_w = wide(sddl);
    let mut descriptor = ptr::null_mut();
    // SAFETY: the security descriptor is allocated by Windows for this SDDL.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl_w.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl = ptr::null_mut();
    // SAFETY: descriptor remains live until after the DACL is installed.
    let result =
        unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) };
    let outcome = if result == 0 {
        Err(io::Error::last_os_error())
    } else if present == 0 || dacl.is_null() {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "runner DACL is missing",
        ))
    } else {
        let mut path = wide_path(path);
        // SAFETY: root path and DACL are valid for SetNamedSecurityInfoW.
        let status = unsafe {
            SetNamedSecurityInfoW(
                path.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                dacl,
                ptr::null(),
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    };
    // SAFETY: descriptor was allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
    unsafe {
        LocalFree(descriptor);
    }
    outcome
}

fn current_user_sid() -> io::Result<String> {
    let mut token = ptr::null_mut();
    // SAFETY: GetCurrentProcess is always valid and TOKEN_QUERY is read-only.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut length = 0;
    unsafe {
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut length);
    }
    let mut bytes = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: buffer is sized by the preceding Windows query.
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            bytes.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    };
    let error = io::Error::last_os_error();
    unsafe {
        CloseHandle(token);
    }
    if result == 0 {
        return Err(error);
    }
    let user = unsafe { &*(bytes.as_ptr() as *const TOKEN_USER) };
    sid_string(user.User.Sid)
}

fn account_sid(user: &str) -> io::Result<String> {
    let name = wide(user);
    let mut sid_len = 0;
    let mut domain_len = 0;
    let mut kind: SID_NAME_USE = 0;
    unsafe {
        LookupAccountNameW(
            ptr::null(),
            name.as_ptr(),
            ptr::null_mut(),
            &mut sid_len,
            ptr::null_mut(),
            &mut domain_len,
            &mut kind,
        );
    }
    let mut sid = vec![0usize; (sid_len as usize).div_ceil(std::mem::size_of::<usize>())];
    let mut domain = vec![0u16; domain_len as usize];
    // SAFETY: buffers are sized by the preceding Windows query.
    if unsafe {
        LookupAccountNameW(
            ptr::null(),
            name.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut kind,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    sid_string(sid.as_mut_ptr().cast())
}

fn sid_string(sid: *mut c_void) -> io::Result<String> {
    let mut ptr_sid = ptr::null_mut();
    // SAFETY: SID comes from a Windows token or LookupAccountNameW.
    if unsafe { ConvertSidToStringSidW(sid, &mut ptr_sid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let length = (0..256).find(|&i| unsafe { *ptr_sid.add(i) == 0 });
    let Some(length) = length else {
        unsafe {
            LocalFree(ptr_sid.cast());
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SID string is too long",
        ));
    };
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr_sid, length) });
    unsafe {
        LocalFree(ptr_sid.cast());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{process_path, runner_password, separate_roots, windows_command_line, RUNNER_USER};
    use std::path::Path;
    #[test]
    fn runner_account_name_fits_windows_local_user_limit() {
        assert!(RUNNER_USER.encode_utf16().count() <= 20);
    }
    #[test]
    fn process_directory_uses_a_windows_path_apps_can_open() {
        assert_eq!(
            process_path(Path::new(r"\\?\C:\ProgramData\jobs\one")).unwrap(),
            Path::new(r"C:\ProgramData\jobs\one")
        );
        assert_eq!(
            process_path(Path::new(r"\\?\UNC\server\share\one")).unwrap(),
            Path::new(r"\\server\share\one")
        );
        assert!(process_path(Path::new(r"\\?\Volume{abc}\jobs")).is_err());
    }
    #[test]
    fn generated_password_has_complexity_suffix() {
        let password = runner_password(&[7u8; 28]);
        assert_eq!(password.len(), 32);
        assert!(password.ends_with("Aa9#"));
    }
    #[test]
    fn runner_root_cannot_expose_agent_state() {
        assert!(separate_roots(
            Path::new(r"C:\Users\Alice\Agent"),
            Path::new(r"C:\ProgramData\Protectorate\Runner")
        )
        .is_ok());
        assert!(separate_roots(Path::new(r"C:\Agent"), Path::new(r"c:\agent\jobs")).is_err());
        assert!(separate_roots(Path::new(r"C:\Agent\state"), Path::new(r"c:\agent")).is_err());
    }
    #[test]
    fn command_line_quotes_windows_arguments() {
        assert_eq!(
            windows_command_line(&["tool.exe".into(), "two words".into(), r#"a\"b"#.into()]),
            r#"tool.exe "two words" "a\\\"b""#
        );
    }
}

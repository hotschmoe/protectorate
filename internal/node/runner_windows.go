//go:build windows

package node

import (
	"crypto/rand"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
	"golang.org/x/sys/windows/registry"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/fsutil"
)

var (
	modadvapi32                 = windows.NewLazySystemDLL("advapi32.dll")
	procLogonUserW              = modadvapi32.NewProc("LogonUserW")
	procCreateProcessWithLogonW = modadvapi32.NewProc("CreateProcessWithLogonW")
	modnetapi32                 = windows.NewLazySystemDLL("netapi32.dll")
	procNetUserAdd              = modnetapi32.NewProc("NetUserAdd")
	procNetUserSetInfo          = modnetapi32.NewProc("NetUserSetInfo")
	procNetLocalGroupAddMembers = modnetapi32.NewProc("NetLocalGroupAddMembers")
)

const (
	logon32LogonInteractive = 2
	logon32ProviderDefault  = 0
	logonWithProfile        = 0x1

	userPrivUser        = 1
	ufScript            = 0x0001
	ufDontExpirePasswd  = 0x10000
	ufPasswdCantChange  = 0x0040
	nerrUserExists      = 2224
	errorMemberInAlias  = 1378
	userInfoLevel1      = 1
	userInfoLevel1003   = 1003
	localGroupMembers3  = 3
	hiddenUsersRegistry = `SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\SpecialAccounts\UserList`
)

// sandbox runs jobs as the runner account: CreateProcessWithLogonW, the
// documented way for a process without SeAssignPrimaryTokenPrivilege (the
// agent runs as a plain user, not as a service) to start a process as
// another user, into a job object of the agent's.
type sandbox struct {
	user, password, jobDir string
}

func (j *Jobs) newSandbox() (*sandbox, error) {
	if j.Home == "" {
		return nil, errors.New("the agent has no state directory")
	}
	c, err := readRunnerConfig(j.Home)
	if errors.Is(err, os.ErrNotExist) {
		return nil, errors.New("no runner account: node install --runner adds one")
	}
	if err != nil {
		return nil, err
	}
	if !within(c.JobDir, j.dir) {
		return nil, fmt.Errorf("the runner shares %s but jobs run in %s", c.JobDir, j.dir)
	}
	pw, err := unseal(c.Password)
	if err != nil {
		return nil, fmt.Errorf("unseal the runner password (sealed for another user?): %w", err)
	}
	tok, err := logonUser(c.User, pw)
	if err != nil {
		return nil, fmt.Errorf("sign in as %s: %w", c.User, err)
	}
	tok.Close()
	return &sandbox{user: c.User, password: pw, jobDir: c.JobDir}, nil
}

// plan resolves argv[0] the way the job's agent would and checks the
// working directory is one the runner shares.
func (s *sandbox) plan(req *pb.RunJobRequest, workdir, _ string, _ [][2]string) ([]string, []byte, error) {
	if !within(s.jobDir, workdir) {
		return nil, nil, fmt.Errorf("working directory %s is outside the runner's %s", workdir, s.jobDir)
	}
	exe := req.Command[0]
	if strings.ContainsAny(exe, `/\`) && !filepath.IsAbs(exe) {
		exe = filepath.Join(workdir, exe)
	}
	path, err := exec.LookPath(exe)
	if err != nil {
		return nil, nil, err
	}
	return append([]string{path}, req.Command[1:]...), nil, nil
}

func logonUser(user, password string) (windows.Token, error) {
	u, err := windows.UTF16PtrFromString(user)
	if err != nil {
		return 0, err
	}
	dom, _ := windows.UTF16PtrFromString(".")
	p, err := windows.UTF16PtrFromString(password)
	if err != nil {
		return 0, err
	}
	var tok windows.Token
	r, _, e := procLogonUserW.Call(uintptr(unsafe.Pointer(u)), uintptr(unsafe.Pointer(dom)), uintptr(unsafe.Pointer(p)),
		logon32LogonInteractive, logon32ProviderDefault, uintptr(unsafe.Pointer(&tok)))
	if r == 0 {
		return 0, e
	}
	return tok, nil
}

// runnerEnv is the runner's own environment, from its profile, with the
// job's variables on top: nothing of the agent's.
func (s *sandbox) runnerEnv(job [][2]string) ([]uint16, error) {
	tok, err := logonUser(s.user, s.password)
	if err != nil {
		return nil, fmt.Errorf("sign in as %s: %w", s.user, err)
	}
	defer tok.Close()
	var block *uint16
	if err := windows.CreateEnvironmentBlock(&block, tok, false); err != nil {
		return nil, fmt.Errorf("runner environment: %w", err)
	}
	defer windows.DestroyEnvironmentBlock(block)
	var raw []uint16
	for p := unsafe.Pointer(block); ; p = unsafe.Add(p, 2) {
		c := *(*uint16)(p)
		raw = append(raw, c)
		if c == 0 && len(raw) > 1 && raw[len(raw)-2] == 0 {
			break
		}
	}
	return envBlock(mergeEnv(splitEnvBlock(raw), job)), nil
}

// runnerRunning is a job started as the runner.
type runnerRunning struct {
	job, process windows.Handle
	out          *os.File
	copied       chan struct{}
}

func (r *runnerRunning) wait() int {
	_, _ = windows.WaitForSingleObject(r.process, windows.INFINITE)
	select {
	case <-r.copied:
	case <-time.After(waitDelay):
		r.out.Close()
	}
	var code uint32
	if err := windows.GetExitCodeProcess(r.process, &code); err != nil {
		return -1
	}
	return int(code)
}

func (r *runnerRunning) interrupt() bool { return false }

func (r *runnerRunning) kill() { _ = windows.TerminateJobObject(r.job, 1) }

func (r *runnerRunning) close() {
	windows.CloseHandle(r.process)
	windows.CloseHandle(r.job)
	r.out.Close()
}

// launch starts argv as the runner, suspended, puts it into a job object
// with the job's limits and resumes it. Output goes through an inheritable
// pipe; the console the process gets is hidden.
func (s *sandbox) launch(req *pb.RunJobRequest, argv []string, workdir string, job [][2]string, w io.Writer) (running, error) {
	env, err := s.runnerEnv(job)
	if err != nil {
		return nil, err
	}
	sa := &windows.SecurityAttributes{Length: uint32(unsafe.Sizeof(windows.SecurityAttributes{})), InheritHandle: 1}
	var rd, wr windows.Handle
	if err := windows.CreatePipe(&rd, &wr, sa, 0); err != nil {
		return nil, fmt.Errorf("output pipe: %w", err)
	}
	if err := windows.SetHandleInformation(rd, windows.HANDLE_FLAG_INHERIT, 0); err != nil {
		windows.CloseHandle(rd)
		windows.CloseHandle(wr)
		return nil, err
	}
	nulName, _ := windows.UTF16PtrFromString("NUL")
	nul, err := windows.CreateFile(nulName, windows.GENERIC_READ, windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE, sa, windows.OPEN_EXISTING, 0, 0)
	if err != nil {
		windows.CloseHandle(rd)
		windows.CloseHandle(wr)
		return nil, err
	}
	defer windows.CloseHandle(nul)
	out := os.NewFile(uintptr(rd), "runner-output")
	jobH, err := newJobObject(req.Limits)
	if err != nil {
		windows.CloseHandle(wr)
		out.Close()
		return nil, err
	}
	si := windows.StartupInfo{
		Cb: uint32(unsafe.Sizeof(windows.StartupInfo{})), Flags: windows.STARTF_USESTDHANDLES | windows.STARTF_USESHOWWINDOW,
		ShowWindow: windows.SW_HIDE, StdInput: nul, StdOutput: wr, StdErr: wr,
	}
	var pi windows.ProcessInformation
	err = createProcessWithLogon(s.user, s.password, windowsCommandLine(argv), env, workdir, &si, &pi)
	windows.CloseHandle(wr)
	if err != nil {
		windows.CloseHandle(jobH)
		out.Close()
		return nil, fmt.Errorf("start as %s: %w", s.user, err)
	}
	defer windows.CloseHandle(pi.Thread)
	if err := windows.AssignProcessToJobObject(jobH, pi.Process); err != nil {
		_ = windows.TerminateProcess(pi.Process, 1)
		windows.CloseHandle(pi.Process)
		windows.CloseHandle(jobH)
		out.Close()
		return nil, fmt.Errorf("attach to job object: %w", err)
	}
	if _, err := windows.ResumeThread(pi.Thread); err != nil {
		_ = windows.TerminateJobObject(jobH, 1)
		windows.CloseHandle(pi.Process)
		windows.CloseHandle(jobH)
		out.Close()
		return nil, fmt.Errorf("resume: %w", err)
	}
	r := &runnerRunning{job: jobH, process: pi.Process, out: out, copied: make(chan struct{})}
	go func() {
		defer close(r.copied)
		_, _ = io.Copy(w, out)
	}()
	return r, nil
}

func createProcessWithLogon(user, password, cmdline string, env []uint16, dir string, si *windows.StartupInfo, pi *windows.ProcessInformation) error {
	u, err := windows.UTF16PtrFromString(user)
	if err != nil {
		return err
	}
	dom, _ := windows.UTF16PtrFromString(".")
	p, err := windows.UTF16PtrFromString(password)
	if err != nil {
		return err
	}
	cl, err := windows.UTF16FromString(cmdline)
	if err != nil {
		return err
	}
	d, err := windows.UTF16PtrFromString(dir)
	if err != nil {
		return err
	}
	const flags = windows.CREATE_SUSPENDED | windows.CREATE_UNICODE_ENVIRONMENT | windows.CREATE_NEW_CONSOLE
	r, _, e := procCreateProcessWithLogonW.Call(uintptr(unsafe.Pointer(u)), uintptr(unsafe.Pointer(dom)), uintptr(unsafe.Pointer(p)),
		logonWithProfile, 0, uintptr(unsafe.Pointer(&cl[0])), flags, uintptr(unsafe.Pointer(&env[0])),
		uintptr(unsafe.Pointer(d)), uintptr(unsafe.Pointer(si)), uintptr(unsafe.Pointer(pi)))
	if r == 0 {
		return e
	}
	return nil
}

func seal(secret string) ([]byte, error) {
	in := windows.DataBlob{Size: uint32(len(secret)), Data: unsafe.StringData(secret)}
	ent := windows.DataBlob{Size: uint32(len(runnerEntropy)), Data: unsafe.StringData(runnerEntropy)}
	var out windows.DataBlob
	if err := windows.CryptProtectData(&in, nil, &ent, 0, nil, windows.CRYPTPROTECT_UI_FORBIDDEN, &out); err != nil {
		return nil, err
	}
	defer windows.LocalFree(windows.Handle(unsafe.Pointer(out.Data)))
	return append([]byte(nil), unsafe.Slice(out.Data, out.Size)...), nil
}

func unseal(blob []byte) (string, error) {
	in := windows.DataBlob{Size: uint32(len(blob)), Data: &blob[0]}
	ent := windows.DataBlob{Size: uint32(len(runnerEntropy)), Data: unsafe.StringData(runnerEntropy)}
	var out windows.DataBlob
	if err := windows.CryptUnprotectData(&in, nil, &ent, 0, nil, windows.CRYPTPROTECT_UI_FORBIDDEN, &out); err != nil {
		return "", err
	}
	defer windows.LocalFree(windows.Handle(unsafe.Pointer(out.Data)))
	return string(unsafe.Slice(out.Data, out.Size)), nil
}

// netErr turns a NET_API_STATUS into an error, nil for success.
func netErr(r uintptr) error {
	if r == 0 {
		return nil
	}
	return windows.Errno(r)
}

type userInfo1 struct {
	Name, Password    *uint16
	PasswordAge, Priv uint32
	HomeDir, Comment  *uint16
	Flags             uint32
	ScriptPath        *uint16
}

// ensureRunnerAccount creates the runner as a standard local user, or
// resets the password of an existing one.
func ensureRunnerAccount(user, password string) error {
	name, _ := windows.UTF16PtrFromString(user)
	pw, err := windows.UTF16PtrFromString(password)
	if err != nil {
		return err
	}
	comment, _ := windows.UTF16PtrFromString("runs protectorate jobs; created by node install --runner")
	ui := userInfo1{Name: name, Password: pw, Priv: userPrivUser, Comment: comment, Flags: ufScript | ufDontExpirePasswd | ufPasswdCantChange}
	r, _, _ := procNetUserAdd.Call(0, userInfoLevel1, uintptr(unsafe.Pointer(&ui)), 0)
	if r == nerrUserExists {
		r, _, _ = procNetUserSetInfo.Call(0, uintptr(unsafe.Pointer(name)), userInfoLevel1003, uintptr(unsafe.Pointer(&pw)), 0)
	}
	if err := netErr(r); err != nil {
		return fmt.Errorf("create account %s: %w", user, err)
	}
	users, err := windows.CreateWellKnownSid(windows.WinBuiltinUsersSid)
	if err != nil {
		return err
	}
	group, _, _, err := users.LookupAccount("")
	if err != nil {
		return fmt.Errorf("name of the Users group: %w", err)
	}
	g, _ := windows.UTF16PtrFromString(group)
	member := name
	r, _, _ = procNetLocalGroupAddMembers.Call(0, uintptr(unsafe.Pointer(g)), localGroupMembers3, uintptr(unsafe.Pointer(&member)), 1)
	if r != errorMemberInAlias {
		if err := netErr(r); err != nil {
			return fmt.Errorf("add %s to %s: %w", user, group, err)
		}
	}
	return nil
}

func hideFromSignIn(user string) error {
	k, _, err := registry.CreateKey(registry.LOCAL_MACHINE, hiddenUsersRegistry, registry.SET_VALUE)
	if err != nil {
		return fmt.Errorf("open %s: %w", hiddenUsersRegistry, err)
	}
	defer k.Close()
	return k.SetDWordValue(user, 0)
}

func shareJobDir(dir, user string) error {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	tu, err := windows.GetCurrentProcessToken().GetTokenUser()
	if err != nil {
		return err
	}
	runner, _, _, err := windows.LookupSID("", user)
	if err != nil {
		return fmt.Errorf("look up %s: %w", user, err)
	}
	sd, err := windows.SecurityDescriptorFromString(runnerDirSDDL(tu.User.Sid.String(), runner.String()))
	if err != nil {
		return err
	}
	dacl, _, err := sd.DACL()
	if err != nil {
		return err
	}
	return windows.SetNamedSecurityInfo(dir, windows.SE_FILE_OBJECT,
		windows.DACL_SECURITY_INFORMATION|windows.PROTECTED_DACL_SECURITY_INFORMATION, nil, nil, dacl, nil)
}

// InstallRunner creates the runner account and the job root it shares with
// the agent, and seals its password for this user. It must run elevated, as
// the user the agent runs as; each step is printed before it runs.
func InstallRunner(home string, out io.Writer) error {
	dir := RunnerDir()
	steps := RunnerSteps(home, dir)
	step := func(i int) { fmt.Fprintf(out, "%d. %s\n", i+1, steps[i]) }
	step(0)
	if !windows.GetCurrentProcessToken().IsElevated() {
		return errors.New("run this from a terminal opened with Run as administrator, signed in as the user the agent runs as")
	}
	step(1)
	random := make([]byte, 28)
	if _, err := rand.Read(random); err != nil {
		return err
	}
	pw, err := runnerPassword(random)
	if err != nil {
		return err
	}
	step(2)
	if err := ensureRunnerAccount(RunnerUser, pw); err != nil {
		return err
	}
	step(3)
	if err := hideFromSignIn(RunnerUser); err != nil {
		return err
	}
	step(4)
	if err := shareJobDir(dir, RunnerUser); err != nil {
		return fmt.Errorf("share %s: %w", dir, err)
	}
	step(5)
	tok, err := logonUser(RunnerUser, pw)
	if err != nil {
		return fmt.Errorf("sign in as %s: %w", RunnerUser, err)
	}
	tok.Close()
	step(6)
	blob, err := seal(pw)
	if err != nil {
		return fmt.Errorf("seal the password: %w", err)
	}
	raw, err := json.MarshalIndent(runnerConfig{User: RunnerUser, JobDir: dir, Password: blob}, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(home, 0o700); err != nil {
		return err
	}
	if err := fsutil.WriteFile(filepath.Join(home, runnerFile), raw, 0o600); err != nil {
		return err
	}
	step(7)
	return nil
}

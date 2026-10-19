//go:build windows

package node

import (
	"fmt"
	"os/exec"
	"syscall"
	"unsafe"

	"golang.org/x/sys/windows"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

var ntResumeProcess = windows.NewLazySystemDLL("ntdll.dll").NewProc("NtResumeProcess")

// procTree is a started command inside a job object that kills every member
// when terminated or closed.
type procTree struct{ job windows.Handle }

// startTree starts cmd suspended, puts it in a new job object and resumes it,
// so no descendant can be spawned outside the job.
func startTree(cmd *exec.Cmd) (*procTree, error) {
	job, err := windows.CreateJobObject(nil, nil)
	if err != nil {
		return nil, fmt.Errorf("create job object: %w", err)
	}
	var info windows.JOBOBJECT_EXTENDED_LIMIT_INFORMATION
	info.BasicLimitInformation.LimitFlags = windows.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
	if _, err := windows.SetInformationJobObject(job, windows.JobObjectExtendedLimitInformation,
		uintptr(unsafe.Pointer(&info)), uint32(unsafe.Sizeof(info))); err != nil {
		windows.CloseHandle(job)
		return nil, fmt.Errorf("configure job object: %w", err)
	}
	cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: windows.CREATE_SUSPENDED | windows.CREATE_NO_WINDOW}
	if err := cmd.Start(); err != nil {
		windows.CloseHandle(job)
		return nil, err
	}
	h, err := windows.OpenProcess(windows.PROCESS_SET_QUOTA|windows.PROCESS_TERMINATE|windows.PROCESS_SUSPEND_RESUME, false, uint32(cmd.Process.Pid))
	if err == nil {
		defer windows.CloseHandle(h)
		if err = windows.AssignProcessToJobObject(job, h); err == nil {
			if r, _, e := ntResumeProcess.Call(uintptr(h)); r != 0 {
				err = fmt.Errorf("resume process: status %#x: %w", r, e)
			}
		}
	}
	if err != nil {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		windows.CloseHandle(job)
		return nil, fmt.Errorf("attach process to job object: %w", err)
	}
	return &procTree{job: job}, nil
}

// interrupt reports false: Windows has no polite signal for a process tree.
func (t *procTree) interrupt() bool { return false }

func (t *procTree) kill() { _ = windows.TerminateJobObject(t.job, 1) }

func (t *procTree) close() { _ = windows.CloseHandle(t.job) }

// StartDetached starts exe with no console, in its own process group and
// outside this process's job object where Windows allows, so it outlives this
// process. It does not wait.
func StartDetached(exe string, args []string) error {
	const flags = windows.DETACHED_PROCESS | windows.CREATE_NEW_PROCESS_GROUP | windows.CREATE_NO_WINDOW
	cmd := exec.Command(exe, args...)
	cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: flags | windows.CREATE_BREAKAWAY_FROM_JOB}
	err := cmd.Start()
	if err != nil {
		cmd = exec.Command(exe, args...)
		cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: flags}
		err = cmd.Start()
	}
	if err != nil {
		return err
	}
	return cmd.Process.Release()
}

// probeIsolation reports how this node runs jobs: in a job object under the
// agent's own account, elevated when the agent is.
func probeIsolation() *pb.JobIsolation {
	iso := &pb.JobIsolation{Sandbox: "job-object", Note: "unisolated: jobs run as the agent's account in a job object"}
	if windows.GetCurrentProcessToken().IsElevated() {
		iso.Admin = "elevated"
	} else {
		iso.Note += "; no admin jobs: the agent does not run elevated"
	}
	return iso
}

// adminArgv runs command with the agent's own elevated token.
func adminArgv(admin string, command, _ []string) ([]string, error) {
	if admin != "elevated" {
		return nil, fmt.Errorf("unknown elevation %q", admin)
	}
	return command, nil
}

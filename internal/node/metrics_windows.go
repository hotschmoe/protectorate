package node

import (
	"context"
	"fmt"
	"os"
	"runtime"
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	metricsKernel    = windows.NewLazySystemDLL("kernel32.dll")
	systemTimes      = metricsKernel.NewProc("GetSystemTimes")
	memoryStatus     = metricsKernel.NewProc("GlobalMemoryStatusEx")
	tickCount        = metricsKernel.NewProc("GetTickCount64")
	enumPageFiles    = windows.NewLazySystemDLL("psapi.dll").NewProc("EnumPageFilesW")
	pageFileCallback = windows.NewCallback(collectPageFile)
)

type pageFileCollection struct {
	ctx    context.Context
	result memoryInfo
}

func collectPageFile(param, info, _ uintptr) uintptr {
	c := (*pageFileCollection)(unsafe.Pointer(param))
	if c.ctx.Err() != nil {
		return 0
	}
	s := (*struct {
		Size, Reserved    uint32
		Total, Used, Peak uintptr
	})(unsafe.Pointer(info))
	c.result.Total += uint64(s.Total) * uint64(os.Getpagesize())
	c.result.Used += uint64(s.Used) * uint64(os.Getpagesize())
	return 1
}

func readCPUTimes(ctx context.Context) ([]cpuTimes, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	var idle, kernel, user windows.Filetime
	ok, _, err := systemTimes.Call(uintptr(unsafe.Pointer(&idle)), uintptr(unsafe.Pointer(&kernel)), uintptr(unsafe.Pointer(&user)))
	if ok == 0 {
		return nil, fmt.Errorf("GetSystemTimes: %w", err)
	}
	value := func(t windows.Filetime) float64 { return float64(uint64(t.HighDateTime)<<32 | uint64(t.LowDateTime)) }
	i, k, u := value(idle), value(kernel), value(user)
	return []cpuTimes{{Idle: i, System: k - i, User: u}}, nil
}

type windowsMemoryStatus struct {
	Length, Load                                                                                         uint32
	TotalPhys, AvailPhys, TotalPageFile, AvailPageFile, TotalVirtual, AvailVirtual, AvailExtendedVirtual uint64
}

func getMemoryStatus(ctx context.Context) (windowsMemoryStatus, error) {
	var s windowsMemoryStatus
	if err := ctx.Err(); err != nil {
		return s, err
	}
	s.Length = uint32(unsafe.Sizeof(s))
	ok, _, err := memoryStatus.Call(uintptr(unsafe.Pointer(&s)))
	if ok == 0 {
		return s, fmt.Errorf("GlobalMemoryStatusEx: %w", err)
	}
	return s, nil
}

func readMemory(ctx context.Context) (memoryInfo, error) {
	s, err := getMemoryStatus(ctx)
	return memoryInfo{Total: s.TotalPhys, Available: s.AvailPhys}, err
}

func readSwap(ctx context.Context) (memoryInfo, error) {
	if err := ctx.Err(); err != nil {
		return memoryInfo{}, err
	}
	c := &pageFileCollection{ctx: ctx}
	ok, _, err := enumPageFiles.Call(pageFileCallback, uintptr(unsafe.Pointer(c)))
	runtime.KeepAlive(c)
	if ctx.Err() != nil {
		return memoryInfo{}, ctx.Err()
	}
	if ok == 0 {
		return memoryInfo{}, fmt.Errorf("EnumPageFilesW: %w", err)
	}
	return c.result, nil
}

func hostInfo(ctx context.Context) (hostDetails, error) {
	if err := ctx.Err(); err != nil {
		return hostDetails{}, err
	}
	v := windows.RtlGetVersion()
	u, _, _ := tickCount.Call()
	host, _ := os.Hostname()
	return hostDetails{Platform: "windows", PlatformVersion: fmt.Sprintf("%d.%d.%d", v.MajorVersion, v.MinorVersion, v.BuildNumber), Hostname: host, Uptime: uint64(u) / 1000}, nil
}

func loadAverage(context.Context) (loadValues, error) {
	return loadValues{}, fmt.Errorf("load averages unavailable on Windows")
}

func partitions(ctx context.Context) ([]partition, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	n, err := windows.GetLogicalDriveStrings(0, nil)
	if err != nil {
		return nil, fmt.Errorf("list drives: %w", err)
	}
	buf := make([]uint16, n+1)
	if _, err := windows.GetLogicalDriveStrings(uint32(len(buf)), &buf[0]); err != nil {
		return nil, fmt.Errorf("read drives: %w", err)
	}
	var out []partition
	for start := 0; start < len(buf) && buf[start] != 0; {
		end := start
		for end < len(buf) && buf[end] != 0 {
			end++
		}
		root := windows.UTF16ToString(buf[start:end])
		var fs [256]uint16
		if err := windows.GetVolumeInformation(&buf[start], nil, 0, nil, nil, nil, &fs[0], uint32(len(fs))); err == nil {
			out = append(out, partition{Mountpoint: root, Device: root, Fstype: windows.UTF16ToString(fs[:])})
		}
		start = end + 1
	}
	return out, nil
}

func diskUsage(ctx context.Context, path string) (diskInfo, error) {
	if err := ctx.Err(); err != nil {
		return diskInfo{}, err
	}
	p, err := windows.UTF16PtrFromString(path)
	if err != nil {
		return diskInfo{}, fmt.Errorf("encode drive: %w", err)
	}
	var available, total, free uint64
	if err := windows.GetDiskFreeSpaceEx(p, &available, &total, &free); err != nil {
		return diskInfo{}, fmt.Errorf("read drive %s: %w", path, err)
	}
	v := diskInfo{Total: total, Free: available, Used: total - min(total, free)}
	if v.Used+v.Free > 0 {
		v.UsedPercent = float64(v.Used) / float64(v.Used+v.Free) * 100
	}
	return v, nil
}

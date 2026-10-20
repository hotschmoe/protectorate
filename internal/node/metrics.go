package node

type cpuTimes struct {
	User, Nice, System, Idle, Iowait, Irq, Softirq, Steal float64
}

func (t cpuTimes) Total() float64 {
	return t.User + t.Nice + t.System + t.Idle + t.Iowait + t.Irq + t.Softirq + t.Steal
}

type partition struct {
	Device, Mountpoint, Fstype string
}

type memoryInfo struct {
	Total, Available, Used uint64
}

type diskInfo struct {
	Total, Used, Free uint64
	UsedPercent       float64
}

type hostDetails struct {
	Platform, PlatformVersion, Hostname string
	Uptime                              uint64
}

type loadValues struct{ Load1, Load5, Load15 float64 }

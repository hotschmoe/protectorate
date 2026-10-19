package envoy

import (
	"bytes"
	"fmt"
	"html"
	"sort"
	"time"

	"github.com/hotschmoe/protectorate/internal/protocol"
)

// formatUptime formats a duration as HH:MM:SS for display
func formatUptime(d time.Duration) string {
	if d < 0 {
		d = 0
	}
	h := int(d.Hours())
	m := int(d.Minutes()) % 60
	s := int(d.Seconds()) % 60
	return fmt.Sprintf("%02d:%02d:%02d", h, m, s)
}

// RenderSleeveGrid renders the complete sleeve grid HTML
func RenderSleeveGrid(sleeves []*protocol.SleeveInfo) string {
	if len(sleeves) == 0 {
		return `<div class="empty-state">No sleeves running. Click "+ SPAWN SLEEVE" to create one.</div>`
	}

	// Sort by workspace name
	sorted := make([]*protocol.SleeveInfo, len(sleeves))
	copy(sorted, sleeves)
	sort.Slice(sorted, func(i, j int) bool {
		return getWorkspaceDisplayName(sorted[i].Workspace) < getWorkspaceDisplayName(sorted[j].Workspace)
	})

	var buf bytes.Buffer
	for _, s := range sorted {
		buf.WriteString(renderSleeveCardInternal(s, false))
	}
	return buf.String()
}

// RenderSleeveCard renders a single sleeve card HTML
func RenderSleeveCard(sleeve *protocol.SleeveInfo) string {
	return renderSleeveCardInternal(sleeve, false)
}

// RenderSleeveCardOOB renders a sleeve card with htmx out-of-band swap attribute
func RenderSleeveCardOOB(sleeve *protocol.SleeveInfo) string {
	return renderSleeveCardInternal(sleeve, true)
}

func renderSleeveCardInternal(s *protocol.SleeveInfo, oob bool) string {
	wsName := getWorkspaceDisplayName(s.Workspace)
	uptime := formatUptime(time.Since(s.SpawnTime))
	integrity := s.Integrity
	if integrity == 0 {
		integrity = 100.0
	}

	constraintLabel := "UNCONSTRAINED"
	constraintClass := "unconstrained"
	if s.Constrained {
		constraintLabel = "CONSTRAINED"
		constraintClass = "constrained"
	}

	healthClass := getHealthClass(integrity)

	dhfName := s.DHF
	if dhfName == "" {
		dhfName = "Claude Code"
	}
	dhfVersion := ""
	if s.DHFVersion != "" {
		dhfVersion = " v" + s.DHFVersion
	}
	dhfSuffix := ""
	if !s.SidecarHealthy {
		dhfSuffix = " (cached)"
	}
	dhfDisplay := dhfName + dhfVersion + dhfSuffix

	sidecarClass := "healthy"
	sidecarTitle := "Sidecar connected"
	if !s.SidecarHealthy {
		sidecarClass = "unhealthy"
		sidecarTitle = "Sidecar unreachable"
	}

	memDisplay, memPct, cpuDisplay, cpuPct := formatSleeveResourcesServer(s)

	oobAttr := ""
	if oob {
		oobAttr = fmt.Sprintf(` hx-swap-oob="outerHTML:#sleeve-%s"`, html.EscapeString(s.Name))
	}

	return fmt.Sprintf(`<div id="sleeve-%s" class="sleeve-card %s"%s>
    <div class="sleeve-header">
        <span class="sleeve-name">SLEEVE: %s</span>
        <span class="sidecar-status %s" title="%s"></span>
        <span class="sleeve-status active">ACTIVE</span>
    </div>
    <div class="sleeve-body">
        <div class="sleeve-row">
            <span class="sleeve-label">DHF</span>
            <span class="sleeve-value">%s</span>
        </div>
        <div class="sleeve-row">
            <span class="sleeve-label">Workspace</span>
            <span class="sleeve-value">%s</span>
        </div>
        <div class="sleeve-row">
            <span class="sleeve-label">Uptime</span>
            <span class="sleeve-value">%s</span>
        </div>
        <div class="sleeve-row">
            <span class="sleeve-label">Resources</span>
            <span class="sleeve-value %s">%s</span>
        </div>

        <div class="integrity-bar">
            <div class="integrity-label">
                <span class="sleeve-label">Stack Integrity</span>
                <span class="sleeve-value">%.1f%%</span>
            </div>
            <div class="integrity-track">
                <div class="integrity-fill" style="width: %.1f%%"></div>
            </div>
        </div>

        <div class="resource-row">
            <div class="resource">
                <div class="resource-header">
                    <span class="resource-label">MEMORY</span>
                    <span class="resource-value">%s</span>
                </div>
                <div class="resource-bar">
                    <div class="resource-fill" style="width: %d%%"></div>
                </div>
            </div>
            <div class="resource">
                <div class="resource-header">
                    <span class="resource-label">CPU</span>
                    <span class="resource-value">%s</span>
                </div>
                <div class="resource-bar">
                    <div class="resource-fill" style="width: %d%%"></div>
                </div>
            </div>
        </div>
    </div>
    <div class="sleeve-actions">
        <button class="btn" onclick="openTerminal('%s')">TERMINAL</button>
        <button class="btn" onclick="openTerminalObserve('%s')">OBSERVE</button>
        <button class="btn btn-danger" onclick="killSleeve('%s')">KILL</button>
    </div>
</div>`,
		html.EscapeString(s.Name),
		healthClass,
		oobAttr,
		html.EscapeString(s.Name),
		sidecarClass,
		sidecarTitle,
		html.EscapeString(dhfDisplay),
		html.EscapeString(wsName),
		uptime,
		constraintClass,
		constraintLabel,
		integrity,
		integrity,
		memDisplay,
		memPct,
		cpuDisplay,
		cpuPct,
		html.EscapeString(s.Name),
		html.EscapeString(s.Name),
		html.EscapeString(s.Name),
	)
}

// RenderHostStats renders the host stats HTML fragments for OOB swap
func RenderHostStats(stats *protocol.HostStats) string {
	var buf bytes.Buffer

	if stats.CPU != nil {
		cpuPct := int(stats.CPU.UsagePercent)
		buf.WriteString(fmt.Sprintf(
			`<span id="host-cpu-value" hx-swap-oob="innerHTML:#host-cpu-value">%d%%</span>`,
			cpuPct,
		))
		buf.WriteString(fmt.Sprintf(
			`<div id="host-cpu-bar" hx-swap-oob="outerHTML:#host-cpu-bar" class="host-resource-fill" style="width: %d%%"></div>`,
			cpuPct,
		))
		buf.WriteString(fmt.Sprintf(
			`<span id="host-cpu-detail" hx-swap-oob="innerHTML:#host-cpu-detail">%d cores / %d threads</span>`,
			stats.CPU.Cores, stats.CPU.Threads,
		))
	}

	if stats.Memory != nil {
		usedGB := float64(stats.Memory.UsedBytes) / (1024 * 1024 * 1024)
		totalGB := float64(stats.Memory.TotalBytes) / (1024 * 1024 * 1024)
		memPct := int(stats.Memory.Percent)
		buf.WriteString(fmt.Sprintf(
			`<span id="host-memory-value" hx-swap-oob="innerHTML:#host-memory-value">%.1f GB</span>`,
			usedGB,
		))
		buf.WriteString(fmt.Sprintf(
			`<div id="host-memory-bar" hx-swap-oob="outerHTML:#host-memory-bar" class="host-resource-fill" style="width: %d%%"></div>`,
			memPct,
		))
		buf.WriteString(fmt.Sprintf(
			`<span id="host-memory-detail" hx-swap-oob="innerHTML:#host-memory-detail">%.1f / %.1f GB used</span>`,
			usedGB, totalGB,
		))
	}

	if stats.Disk != nil {
		usedGB := stats.Disk.UsedBytes / (1024 * 1024 * 1024)
		totalGB := stats.Disk.TotalBytes / (1024 * 1024 * 1024)
		diskPct := int(stats.Disk.Percent)
		buf.WriteString(fmt.Sprintf(
			`<span id="host-disk-value" hx-swap-oob="innerHTML:#host-disk-value">%d GB</span>`,
			usedGB,
		))
		buf.WriteString(fmt.Sprintf(
			`<div id="host-disk-bar" hx-swap-oob="outerHTML:#host-disk-bar" class="host-resource-fill" style="width: %d%%"></div>`,
			diskPct,
		))
		buf.WriteString(fmt.Sprintf(
			`<span id="host-disk-detail" hx-swap-oob="innerHTML:#host-disk-detail">%d / %d GB used</span>`,
			usedGB, totalGB,
		))
	}

	if stats.Docker != nil {
		dockerPct := 0
		if stats.Docker.MaxContainers > 0 {
			dockerPct = stats.Docker.RunningContainers * 100 / stats.Docker.MaxContainers
		}
		buf.WriteString(fmt.Sprintf(
			`<span id="host-docker-value" hx-swap-oob="innerHTML:#host-docker-value">%d</span>`,
			stats.Docker.RunningContainers,
		))
		buf.WriteString(fmt.Sprintf(
			`<div id="host-docker-bar" hx-swap-oob="outerHTML:#host-docker-bar" class="host-resource-fill" style="width: %d%%"></div>`,
			dockerPct,
		))
		buf.WriteString(fmt.Sprintf(
			`<span id="host-docker-detail" hx-swap-oob="innerHTML:#host-docker-detail">%d containers / %d limit</span>`,
			stats.Docker.RunningContainers, stats.Docker.MaxContainers,
		))
	}

	return buf.String()
}

// RenderSleeveCount renders the sleeve count for OOB swap
func RenderSleeveCount(count int) string {
	return fmt.Sprintf(`<div id="stat-active" hx-swap-oob="innerHTML:#stat-active" class="stat-value">%d</div>`, count)
}

func getWorkspaceDisplayName(workspacePath string) string {
	if workspacePath == "" {
		return "unknown"
	}
	// Extract last path component
	for i := len(workspacePath) - 1; i >= 0; i-- {
		if workspacePath[i] == '/' {
			return workspacePath[i+1:]
		}
	}
	return workspacePath
}

func getHealthClass(integrity float64) string {
	if integrity >= 80 {
		return "healthy"
	}
	if integrity >= 50 {
		return "warning"
	}
	return "critical"
}

func formatSleeveResourcesServer(s *protocol.SleeveInfo) (memDisplay string, memPct int, cpuDisplay string, cpuPct int) {
	resources := s.Resources

	if resources == nil {
		return "-", 0, "-", 0
	}

	memUsedBytes := resources.MemoryUsedBytes
	memUsedGB := float64(memUsedBytes) / (1024 * 1024 * 1024)
	cpuPercent := int(resources.CPUPercent)

	if s.Constrained && s.MemoryLimitMB > 0 {
		memLimitGB := float64(s.MemoryLimitMB) / 1024
		memPct = int(resources.MemoryPercent)
		memDisplay = fmt.Sprintf("%.1f / %.1f GB", memUsedGB, memLimitGB)
	} else {
		memDisplay = fmt.Sprintf("%.1f GB of host", memUsedGB)
		memPct = 0
	}

	if s.Constrained && s.CPULimit > 0 {
		cpuDisplay = fmt.Sprintf("%d%% of %d cores", cpuPercent, s.CPULimit)
	} else {
		cpuDisplay = fmt.Sprintf("%d%% of host", cpuPercent)
	}
	cpuPct = cpuPercent

	return memDisplay, memPct, cpuDisplay, cpuPct
}

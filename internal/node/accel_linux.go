package node

import (
	"encoding/binary"
	"unsafe"

	"golang.org/x/sys/unix"
)

func newAccelProbe() accelProbe { return newSysfsProbe("/sys", renderNodeVRAM) }

// DRM_IOCTL_XE_DEVICE_QUERY and DRM_XE_DEVICE_QUERY_MEM_REGIONS from
// include/uapi/drm/xe_drm.h.
const (
	xeIoctlDeviceQuery  = 0xc0286440
	xeQueryMemRegions   = 1
	xeMemRegionVRAM     = 1
	xeMemRegionSize     = 88
	xeMemRegionsHdrSize = 8
)

type xeDeviceQuery struct {
	extensions uint64
	query      uint32
	size       uint32
	data       uint64
	reserved   [2]uint64
}

// renderNodeVRAM asks an xe GPU for its VRAM size. sysfs does not expose it;
// the query needs only read access to the render node.
func renderNodeVRAM(driver, renderNode string) uint64 {
	if driver != "xe" {
		return 0
	}
	fd, err := unix.Open("/dev/dri/"+renderNode, unix.O_RDONLY|unix.O_CLOEXEC, 0)
	if err != nil {
		return 0
	}
	defer unix.Close(fd)
	q := xeDeviceQuery{query: xeQueryMemRegions}
	if err := xeQuery(fd, &q); err != nil || q.size < xeMemRegionsHdrSize {
		return 0
	}
	buf := make([]byte, q.size)
	q.data = uint64(uintptr(unsafe.Pointer(&buf[0])))
	if err := xeQuery(fd, &q); err != nil {
		return 0
	}
	n := int(binary.LittleEndian.Uint32(buf))
	var total uint64
	for i := 0; i < n; i++ {
		off := xeMemRegionsHdrSize + i*xeMemRegionSize
		if off+xeMemRegionSize > len(buf) {
			break
		}
		if binary.LittleEndian.Uint16(buf[off:]) == xeMemRegionVRAM {
			total += binary.LittleEndian.Uint64(buf[off+8:])
		}
	}
	return total
}

func xeQuery(fd int, q *xeDeviceQuery) error {
	_, _, errno := unix.Syscall(unix.SYS_IOCTL, uintptr(fd), xeIoctlDeviceQuery, uintptr(unsafe.Pointer(q)))
	if errno != 0 {
		return errno
	}
	return nil
}

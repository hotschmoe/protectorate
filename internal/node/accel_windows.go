package node

import (
	"context"
	"encoding/binary"
	"errors"
	"math"
	"unsafe"

	"golang.org/x/sys/windows"
	"golang.org/x/sys/windows/registry"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

var (
	gdi32Accel = windows.NewLazySystemDLL("gdi32.dll")
	pdhDLL     = windows.NewLazySystemDLL("pdh.dll")

	pOpenAdapterFromDeviceName = gdi32Accel.NewProc("D3DKMTOpenAdapterFromDeviceName")
	pQueryAdapterInfo          = gdi32Accel.NewProc("D3DKMTQueryAdapterInfo")
	pCloseAdapter              = gdi32Accel.NewProc("D3DKMTCloseAdapter")

	pPdhOpenQuery         = pdhDLL.NewProc("PdhOpenQueryW")
	pPdhAddEnglishCounter = pdhDLL.NewProc("PdhAddEnglishCounterW")
	pPdhCollectQueryData  = pdhDLL.NewProc("PdhCollectQueryData")
	pPdhGetFormattedArray = pdhDLL.NewProc("PdhGetFormattedCounterArrayW")
	pPdhCloseQuery        = pdhDLL.NewProc("PdhCloseQuery")
)

var (
	displayClassGUID    = windows.GUID{Data1: 0x4d36e968, Data2: 0xe325, Data3: 0x11ce, Data4: [8]byte{0xbf, 0xc1, 0x08, 0x00, 0x2b, 0xe1, 0x03, 0x18}}
	computeAccelGUID    = windows.GUID{Data1: 0xf01a9d53, Data2: 0x3ff6, Data3: 0x48d2, Data4: [8]byte{0x9f, 0x97, 0xc8, 0xa7, 0x00, 0x4b, 0xe1, 0x0c}}
	computeDeviceIfGUID = windows.GUID{Data1: 0x1024e4c9, Data2: 0x47c9, Data3: 0x48d3, Data4: [8]byte{0xb4, 0xa8, 0xf9, 0xdf, 0x78, 0x52, 0x3b, 0x53}}
	displayDeviceIfGUID = windows.GUID{Data1: 0x1ca05180, Data2: 0xa699, Data3: 0x450a, Data4: [8]byte{0x9a, 0x0c, 0xde, 0x4f, 0xbe, 0x3d, 0xdd, 0x89}}
)

func newAccelProbe() accelProbe { return &winProbe{} }

type winProbe struct {
	pdh *pdhQuery
}

func (p *winProbe) inventory(context.Context) []accelDevice {
	devs := listDevices(&displayClassGUID, false)
	devs = append(devs, listDevices(&computeAccelGUID, true)...)
	return winAccels(devs)
}

func (p *winProbe) sample(_ context.Context, devs []accelDevice) ([]*pb.GpuMetrics, []*pb.AcceleratorMetrics) {
	if len(devs) == 0 {
		return nil, nil
	}
	if p.pdh == nil {
		q, err := openPDH(`\GPU Engine(*)\Utilization Percentage`, `\GPU Adapter Memory(*)\Dedicated Usage`)
		if err != nil {
			return winSample(devs, nil, nil)
		}
		p.pdh = q
	}
	vals, err := p.pdh.collect()
	if err != nil {
		p.pdh.close()
		p.pdh = nil
		return winSample(devs, nil, nil)
	}
	var util map[string]float64
	if vals[0] != nil {
		util = adapterUtilization(vals[0])
	}
	return winSample(devs, util, adapterDedicated(vals[1]))
}

func listDevices(class *windows.GUID, npu bool) []winDevice {
	set, err := windows.SetupDiGetClassDevsEx(class, "", 0, windows.DIGCF_PRESENT, 0, "")
	if err != nil {
		return nil
	}
	defer set.Close()
	var out []winDevice
	for i := 0; ; i++ {
		data, err := set.EnumDeviceInfo(i)
		if errors.Is(err, windows.ERROR_NO_MORE_ITEMS) {
			break
		}
		if err != nil {
			continue
		}
		str := func(p windows.SPDRP) string {
			v, _ := set.DeviceRegistryProperty(data, p)
			s, _ := v.(string)
			return s
		}
		num := func(p windows.SPDRP) int64 {
			if v, err := set.DeviceRegistryProperty(data, p); err == nil {
				if u, ok := v.(uint32); ok {
					return int64(u)
				}
			}
			return -1
		}
		d := winDevice{
			npu: npu, desc: str(windows.SPDRP_FRIENDLYNAME), mfg: str(windows.SPDRP_MFG),
			service: str(windows.SPDRP_SERVICE), enumerator: str(windows.SPDRP_ENUMERATOR_NAME),
			bus: num(windows.SPDRP_BUSNUMBER), address: num(windows.SPDRP_ADDRESS),
		}
		if d.desc == "" {
			d.desc = str(windows.SPDRP_DEVICEDESC)
		}
		d.instanceID, _ = set.DeviceInstanceID(data)
		if h, err := set.OpenDevRegKey(data, windows.DICS_FLAG_GLOBAL, 0, windows.DIREG_DRV, windows.KEY_READ); err == nil {
			k := registry.Key(h)
			d.driverVersion, _, _ = k.GetStringValue("DriverVersion")
			if v, _, err := k.GetIntegerValue("HardwareInformation.qwMemorySize"); err == nil {
				d.memBytes = v
			} else if b, _, err := k.GetBinaryValue("HardwareInformation.MemorySize"); err == nil && len(b) >= 4 {
				d.memBytes = uint64(binary.LittleEndian.Uint32(b))
			}
			k.Close()
		}
		d.luid, d.adapterType = openAdapter(d.instanceID)
		out = append(out, d)
	}
	return out
}

type d3dkmtOpenAdapterFromDeviceName struct {
	deviceName *uint16
	adapter    uint32
	luidLow    uint32
	luidHigh   int32
}

type d3dkmtQueryAdapterInfo struct {
	adapter  uint32
	kind     uint32
	data     unsafe.Pointer
	dataSize uint32
}

const kmtqaiAdapterType = 15

// openAdapter finds the WDDM adapter (GPU or MCDM compute device) behind a
// device instance: its LUID, which keys the PDH counters, and its type bits.
func openAdapter(instanceID string) (string, uint32) {
	if instanceID == "" || pOpenAdapterFromDeviceName.Find() != nil {
		return "", 0
	}
	for _, g := range []*windows.GUID{&computeDeviceIfGUID, &displayDeviceIfGUID} {
		paths, err := windows.CM_Get_Device_Interface_List(instanceID, g, windows.CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
		if err != nil {
			continue
		}
		for _, path := range paths {
			name, err := windows.UTF16PtrFromString(path)
			if err != nil {
				continue
			}
			o := d3dkmtOpenAdapterFromDeviceName{deviceName: name}
			if r, _, _ := pOpenAdapterFromDeviceName.Call(uintptr(unsafe.Pointer(&o))); r != 0 {
				continue
			}
			var typ uint32
			q := d3dkmtQueryAdapterInfo{adapter: o.adapter, kind: kmtqaiAdapterType, data: unsafe.Pointer(&typ), dataSize: 4}
			if r, _, _ := pQueryAdapterInfo.Call(uintptr(unsafe.Pointer(&q))); r != 0 {
				typ = 0
			}
			closeArg := o.adapter
			pCloseAdapter.Call(uintptr(unsafe.Pointer(&closeArg)))
			return luidString(o.luidLow, o.luidHigh), typ
		}
	}
	return "", 0
}

const (
	pdhFmtDouble   = 0x00000200
	pdhFmtNoCap100 = 0x00008000
	pdhMoreData    = 0x800007d2
	pdhCStatusOK   = 0
	pdhCStatusNew  = 1
	pdhItemSize    = 24
)

// pdhQuery holds an open PDH query. Rate counters need two collections, so
// the query lives across samples.
type pdhQuery struct {
	query    windows.Handle
	counters []windows.Handle
}

func openPDH(paths ...string) (*pdhQuery, error) {
	if err := pPdhOpenQuery.Find(); err != nil {
		return nil, err
	}
	q := &pdhQuery{}
	if r, _, _ := pPdhOpenQuery.Call(0, 0, uintptr(unsafe.Pointer(&q.query))); r != 0 {
		return nil, windows.Errno(r)
	}
	for _, p := range paths {
		path, err := windows.UTF16PtrFromString(p)
		if err != nil {
			q.close()
			return nil, err
		}
		var c windows.Handle
		if r, _, _ := pPdhAddEnglishCounter.Call(uintptr(q.query), uintptr(unsafe.Pointer(path)), 0, uintptr(unsafe.Pointer(&c))); r != 0 {
			q.close()
			return nil, windows.Errno(r)
		}
		q.counters = append(q.counters, c)
	}
	return q, nil
}

func (q *pdhQuery) close() { pPdhCloseQuery.Call(uintptr(q.query)) }

// collect returns each counter's instances and values; a counter without a
// valid reading yet (a rate counter's first collection) is nil.
func (q *pdhQuery) collect() ([]map[string]float64, error) {
	if r, _, _ := pPdhCollectQueryData.Call(uintptr(q.query)); r != 0 {
		return nil, windows.Errno(r)
	}
	out := make([]map[string]float64, len(q.counters))
	for i, c := range q.counters {
		out[i] = counterArray(c)
	}
	return out, nil
}

// counterArray reads PDH_FMT_COUNTERVALUE_ITEM_W entries {LPWSTR name;
// DWORD status; double value} from one buffer. The name pointers point into
// the same buffer, so they are read as offsets rather than Go pointers.
func counterArray(c windows.Handle) map[string]float64 {
	var size, count uint32
	r, _, _ := pPdhGetFormattedArray.Call(uintptr(c), pdhFmtDouble|pdhFmtNoCap100, uintptr(unsafe.Pointer(&size)), uintptr(unsafe.Pointer(&count)), 0)
	if uint32(r) != pdhMoreData || size == 0 {
		return nil
	}
	buf := make([]byte, size)
	r, _, _ = pPdhGetFormattedArray.Call(uintptr(c), pdhFmtDouble|pdhFmtNoCap100, uintptr(unsafe.Pointer(&size)), uintptr(unsafe.Pointer(&count)), uintptr(unsafe.Pointer(&buf[0])))
	if r != 0 {
		return nil
	}
	base := uint64(uintptr(unsafe.Pointer(&buf[0])))
	out := make(map[string]float64, count)
	valid := false
	for i := 0; i < int(count) && (i+1)*pdhItemSize <= len(buf); i++ {
		item := buf[i*pdhItemSize:]
		status := binary.LittleEndian.Uint32(item[8:])
		if status != pdhCStatusOK && status != pdhCStatusNew {
			continue
		}
		off := binary.LittleEndian.Uint64(item) - base
		if off >= uint64(len(buf)) {
			continue
		}
		name := utf16At(buf[off:])
		out[name] = math.Float64frombits(binary.LittleEndian.Uint64(item[16:]))
		valid = true
	}
	if !valid {
		return nil
	}
	return out
}

func utf16At(b []byte) string {
	var u []uint16
	for i := 0; i+1 < len(b); i += 2 {
		c := binary.LittleEndian.Uint16(b[i:])
		if c == 0 {
			break
		}
		u = append(u, c)
	}
	return windows.UTF16ToString(u)
}

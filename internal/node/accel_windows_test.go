package node

import "testing"

// TestListDevicesWindows reads this machine's Display and ComputeAccelerator
// devices; with -v it prints the records the fixtures in accel_test.go were
// taken from.
func TestListDevicesWindows(t *testing.T) {
	for _, c := range []struct {
		name string
		devs []winDevice
	}{
		{"display", listDevices(&displayClassGUID, false)},
		{"compute accelerator", listDevices(&computeAccelGUID, true)},
	} {
		for _, d := range c.devs {
			t.Logf("%s: %#v", c.name, d)
			if d.instanceID == "" || d.desc == "" {
				t.Errorf("%s device without instance id or description: %#v", c.name, d)
			}
		}
	}
}

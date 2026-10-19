package sidecar

import "testing"

func TestParseVmRSS(t *testing.T) {
	tests := []struct {
		name   string
		status string
		want   int64
	}{
		{
			name: "typical status output",
			status: `Name:	sidecar
VmPeak:	    8192 kB
VmSize:	    8000 kB
VmRSS:	    4096 kB
VmData:	    2048 kB`,
			want: 4096,
		},
		{
			name: "large value",
			status: `Name:	process
VmRSS:	 1234567 kB
VmData:	    2048 kB`,
			want: 1234567,
		},
		{
			name: "no VmRSS line",
			status: `Name:	process
VmSize:	    8000 kB
VmData:	    2048 kB`,
			want: 0,
		},
		{
			name:   "empty string",
			status: "",
			want:   0,
		},
		{
			name: "VmRSS with no value",
			status: `VmRSS:`,
			want:   0,
		},
		{
			name: "VmRSS with invalid value",
			status: `VmRSS:	notanumber kB`,
			want:   0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := parseVmRSS(tt.status)
			if got != tt.want {
				t.Errorf("parseVmRSS() = %d, want %d", got, tt.want)
			}
		})
	}
}

//go:build !windows

package desk

import (
	"fmt"
	"image"
	"runtime"
)

// New returns a driver that reports ErrUnsupported: only Windows is
// implemented.
func New() Desktop { return unsupported{} }

type unsupported struct{}

func (unsupported) Ready() error {
	return fmt.Errorf("%w (%s/%s); only Windows nodes can be driven", ErrUnsupported, runtime.GOOS, runtime.GOARCH)
}
func (u unsupported) Screenshot() (*image.RGBA, error)     { return nil, u.Ready() }
func (u unsupported) Windows() ([]Window, error)           { return nil, u.Ready() }
func (u unsupported) Click(int, int, Button, bool) error   { return u.Ready() }
func (u unsupported) Move(int, int) error                  { return u.Ready() }
func (u unsupported) Type(string) error                    { return u.Ready() }
func (u unsupported) Key(Chord) error                      { return u.Ready() }
func (u unsupported) Run(string, []string) (uint32, error) { return 0, u.Ready() }

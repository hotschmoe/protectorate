//go:build windows

package main

import "errors"

func reexecHub(string, []string) error { return errors.New("hub self-update requires Linux") }

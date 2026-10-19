package envoy

import "embed"

//go:embed web/templates/* web/static/*
var webFS embed.FS
